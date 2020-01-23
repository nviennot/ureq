use crate::http11::{try_parse_http11, write_http11_req};
use crate::{AsyncRead, AsyncWrite, Error};
use futures_util::future::poll_fn;
use futures_util::ready;
use std::future::Future;
use std::io;
use std::mem;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

const RECV_BODY_SIZE: usize = 16_384;
const HEADER_BUF_SIZE: usize = 1024;

pub fn handshake<S>(io: S) -> (SendRequest, Connection<S>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let inner = Arc::new(Mutex::new(Inner::new()));
    let conn = Connection::new(io, inner.clone());
    let send_req = SendRequest::new(inner);
    (send_req, conn)
}

#[derive(Clone)]
pub struct SendRequest {
    inner: Arc<Mutex<Inner>>,
}

impl SendRequest {
    fn new(inner: Arc<Mutex<Inner>>) -> Self {
        SendRequest { inner }
    }

    pub fn send_request(
        self,
        req: http::Request<()>,
        end: bool,
    ) -> Result<(FutureResponse, SendStream), Error> {
        let seq = {
            let mut inner = self.inner.lock().unwrap();
            let seq = Seq(inner.next_seq);
            inner.next_seq += 1;
            let mut req_header = vec![0; HEADER_BUF_SIZE];
            let size = write_http11_req(&req, &mut req_header[..])?;
            req_header.resize(size, 0);
            let task = Task::SendReq(TaskInfo::new(seq), req_header, End(end));
            inner.enqueue(task);
            seq
        };
        let fut_response = FutureResponse::new(self.inner.clone(), seq);
        let send_stream = SendStream::new(self.inner, seq);
        Ok((fut_response, send_stream))
    }
}

pub struct FutureResponse {
    inner: Arc<Mutex<Inner>>,
    seq: Seq,
}

impl FutureResponse {
    fn new(inner: Arc<Mutex<Inner>>, seq: Seq) -> Self {
        FutureResponse { inner, seq }
    }
}

impl Future for FutureResponse {
    type Output = Result<http::Response<RecvStream>, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(err) = inner.get_remote_error() {
            return Poll::Ready(Err(err));
        }
        if let Some(Task::RecvRes(_, buf, waker)) = inner.tasks.get_recv_res(self.seq) {
            if let Some((req, used_bytes)) = try_parse_http11(&buf[..])? {
                assert_eq!(used_bytes, buf.len(), "Used bytes doesn't match buf len");
                let recv_stream = RecvStream::new(self.inner.clone(), self.seq);
                let (parts, _) = req.into_parts();
                Poll::Ready(Ok(http::Response::from_parts(parts, recv_stream)))
            } else {
                mem::replace(waker, cx.waker().clone());
                Poll::Pending
            }
        } else {
            let task = Task::RecvRes(
                TaskInfo::new(self.seq),
                Vec::with_capacity(HEADER_BUF_SIZE),
                cx.waker().clone(),
            );
            inner.enqueue(task);
            Poll::Pending
        }
    }
}

pub struct SendStream {
    inner: Arc<Mutex<Inner>>,
    seq: Seq,
}

impl SendStream {
    fn new(inner: Arc<Mutex<Inner>>, seq: Seq) -> Self {
        SendStream { inner, seq }
    }

    fn poll_can_send_data(&self, cx: &mut Context) -> Poll<Result<(), Error>> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(err) = inner.get_remote_error() {
            return Poll::Ready(Err(err));
        }
        if let Some(err) = inner.assert_can_send_body(self.seq) {
            return Poll::Ready(Err(err));
        }
        if let Some(Task::SendBody(_, _, _, waker)) = inner.tasks.get_send_body(self.seq) {
            waker.replace(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    pub async fn ready(self) -> Result<SendStream, Error> {
        poll_fn(|cx| self.poll_can_send_data(cx)).await?;
        Ok(self)
    }

    pub fn send_data(&self, data: &[u8], end: bool) -> Result<(), Error> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(err) = inner.assert_can_send_body(self.seq) {
            return Err(err);
        }
        let task = Task::SendBody(TaskInfo::new(self.seq), data.to_owned(), End(end), None);
        inner.enqueue(task);
        Ok(())
    }
}

pub struct RecvStream {
    inner: Arc<Mutex<Inner>>,
    seq: Seq,
}

impl RecvStream {
    fn new(inner: Arc<Mutex<Inner>>, seq: Seq) -> Self {
        Self { inner, seq }
    }

    fn poll_for_content(&self, cx: &mut Context, out: &mut [u8]) -> Poll<Result<usize, Error>> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(Task::RecvBody(_, buf, end, waker)) = inner.tasks.get_recv_body(self.seq) {
            mem::replace(waker, cx.waker().clone());
            if buf.is_empty() && !**end {
                Poll::Pending
            } else {
                let max = buf.len().min(out.len());
                (&mut out[0..max]).copy_from_slice(&buf[0..max]);
                let rest = buf.split_off(max);
                mem::replace(buf, rest);
                Poll::Ready(Ok(max))
            }
        } else {
            let task = Task::RecvBody(
                TaskInfo::new(self.seq),
                Vec::with_capacity(RECV_BODY_SIZE),
                End(false),
                cx.waker().clone(),
            );
            inner.enqueue(task);
            Poll::Pending
        }
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        Ok(poll_fn(|cx| self.poll_for_content(cx, buf)).await?)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum State {
    /// Can accept a new request.
    Ready,
    /// After request header is sent, and we can send a body.
    SendBody,
    /// Waiting to receive response header.
    Waiting,
    /// After we received response header and are waiting for a body.
    RecvBody,
    /// If connection failed.
    Closed,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Seq(usize);
#[derive(Clone, Copy, Eq, PartialEq)]
struct End(bool);
#[derive(Clone, Copy, Eq, PartialEq)]
struct TaskId(usize);

impl Deref for Seq {
    type Target = usize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for End {
    type Target = bool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for TaskId {
    type Target = usize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct TaskInfo(Seq, TaskId);

impl TaskInfo {
    fn new(seq: Seq) -> Self {
        TaskInfo(seq, TaskId(0))
    }
}

#[allow(clippy::large_enum_variant)]
enum Task {
    SendReq(TaskInfo, Vec<u8>, End),
    SendBody(TaskInfo, Vec<u8>, End, Option<Waker>),
    RecvRes(TaskInfo, Vec<u8>, Waker),
    RecvBody(TaskInfo, Vec<u8>, End, Waker),
}

impl Task {
    fn task_info(&self) -> &TaskInfo {
        match self {
            Task::SendReq(ti, _, _) => ti,
            Task::SendBody(ti, _, _, _) => ti,
            Task::RecvRes(ti, _, _) => ti,
            Task::RecvBody(ti, _, _, _) => ti,
        }
    }

    fn task_info_mut(&mut self) -> &mut TaskInfo {
        match self {
            Task::SendReq(ti, _, _) => ti,
            Task::SendBody(ti, _, _, _) => ti,
            Task::RecvRes(ti, _, _) => ti,
            Task::RecvBody(ti, _, _, _) => ti,
        }
    }

    fn is_send_req(&self) -> bool {
        if let Task::SendReq(_, _, _) = self {
            return true;
        }
        false
    }

    fn is_send_body(&self) -> bool {
        if let Task::SendBody(_, _, _, _) = self {
            return true;
        }
        false
    }

    fn is_recv_res(&self) -> bool {
        if let Task::RecvRes(_, _, _) = self {
            return true;
        }
        false
    }

    fn is_recv_body(&self) -> bool {
        if let Task::RecvBody(_, _, _, _) = self {
            return true;
        }
        false
    }
}

struct Tasks(usize, Vec<Task>);

impl Tasks {
    fn new() -> Self {
        Tasks(0, vec![])
    }

    fn push(&mut self, mut task: Task) {
        let task_id = self.0;
        self.0 += 1;
        task.task_info_mut().1 = TaskId(task_id);
        self.1.push(task);
    }

    fn remove(&mut self, task_id: TaskId) {
        self.1.retain(|t| t.task_info().1 != task_id);
    }

    fn get_send_req(&mut self, seq: Seq) -> Option<&mut Task> {
        self.1
            .iter_mut()
            .find(|t| t.task_info().0 == seq && t.is_send_req())
    }

    fn get_send_body(&mut self, seq: Seq) -> Option<&mut Task> {
        self.1
            .iter_mut()
            .find(|t| t.task_info().0 == seq && t.is_send_body())
    }

    fn get_recv_res(&mut self, seq: Seq) -> Option<&mut Task> {
        self.1
            .iter_mut()
            .find(|t| t.task_info().0 == seq && t.is_recv_res())
    }

    fn get_recv_body(&mut self, seq: Seq) -> Option<&mut Task> {
        self.1
            .iter_mut()
            .find(|t| t.task_info().0 == seq && t.is_recv_body())
    }
}

struct Inner {
    next_seq: usize,
    cur_seq: usize,
    state: State,
    error: Option<io::Error>,
    tasks: Tasks,
    conn_waker: Option<Waker>,
}

impl Inner {
    fn new() -> Self {
        Inner {
            next_seq: 0,
            cur_seq: 0,
            state: State::Ready,
            error: None,
            tasks: Tasks::new(),
            conn_waker: None,
        }
    }

    fn enqueue(&mut self, task: Task) {
        self.tasks.push(task);
        if let Some(waker) = self.conn_waker.take() {
            waker.wake();
        }
    }

    fn mark_error(&mut self, err: io::Error) {
        self.state = State::Closed;
        self.error = Some(err);
    }

    fn get_remote_error(&mut self) -> Option<Error> {
        if let State::Closed = &mut self.state {
            return Some(Error::Message(self.error.as_ref().unwrap().to_string()));
        }
        None
    }

    fn assert_can_send_body(&self, seq: Seq) -> Option<Error> {
        if self.cur_seq > *seq {
            return Some(Error::Static("Can't send body for old request"));
        }
        if self.state != State::SendBody {
            let message = format!("Can't send body in state: {:?}", self.state);
            return Some(Error::Message(message));
        }
        None
    }
}

pub struct Connection<S> {
    io: S,
    inner: Arc<Mutex<Inner>>,
}

impl<S> Connection<S> {
    fn new(io: S, inner: Arc<Mutex<Inner>>) -> Self {
        Connection { io, inner }
    }
}

impl<S> Future for Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    type Output = io::Result<()>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let self_ = self.get_mut();
        let mut inner = self_.inner.lock().unwrap();
        let cur_seq = Seq(inner.cur_seq);
        'poll: loop {
            match &mut inner.state {
                State::Ready => {
                    let task = inner.tasks.get_send_req(cur_seq);
                    let to_remove = if let Some(Task::SendReq(info, req, end)) = task {
                        let task_id_to_remove = info.1;
                        let res = ready!(Pin::new(&mut self_.io).poll_write(cx, &req[..]));
                        match res {
                            Ok(amount) => {
                                if amount < req.len() {
                                    let rest = req.split_off(amount);
                                    mem::replace(req, rest);
                                    continue 'poll;
                                }
                                if **end {
                                    inner.state = State::Waiting;
                                } else {
                                    inner.state = State::SendBody;
                                }
                            }
                            Err(err) => {
                                inner.mark_error(err);
                            }
                        }
                        task_id_to_remove
                    } else {
                        inner.conn_waker = Some(cx.waker().clone());
                        return Poll::Pending;
                    };
                    inner.tasks.remove(to_remove);
                }
                State::SendBody => {
                    let task = inner.tasks.get_send_req(cur_seq);
                    let to_remove = if let Some(Task::SendBody(info, body, end, waker)) = task {
                        let task_id_to_remove = info.1;
                        let res = ready!(Pin::new(&mut self_.io).poll_write(cx, &body[..]));
                        match res {
                            Ok(amount) => {
                                if amount < body.len() {
                                    let rest = body.split_off(amount);
                                    mem::replace(body, rest);
                                    continue 'poll;
                                }
                                // entire current send_body was sent, waker is for a
                                // someone potentially waiting to send more.
                                if let Some(waker) = waker.take() {
                                    waker.wake();
                                }
                                if **end {
                                    inner.state = State::Waiting;
                                }
                            }
                            Err(err) => {
                                inner.mark_error(err);
                            }
                        }
                        task_id_to_remove
                    } else {
                        inner.conn_waker = Some(cx.waker().clone());
                        return Poll::Pending;
                    };
                    inner.tasks.remove(to_remove);
                }
                State::Waiting => {
                    let task = inner.tasks.get_recv_res(cur_seq);
                    let to_remove = if let Some(Task::RecvRes(info, buf, waker)) = task {
                        let task_id_to_remove = info.1;
                        const END_OF_HEADER: &[u8] = &[b'\r', b'\n', b'\r', b'\n'];
                        let mut end_index = 0;
                        let mut buf_index = 0;
                        let mut tmp = [0_u8; 1];
                        loop {
                            // read one more char
                            let res = ready!(Pin::new(&mut self_.io).poll_read(cx, &mut tmp[..]));
                            match res {
                                Ok(amount) => {
                                    if amount == 0 {
                                        inner.mark_error(io::Error::new(
                                            io::ErrorKind::UnexpectedEof,
                                            "Data end before complete http11 header",
                                        ));
                                        continue 'poll;
                                    }
                                    buf.push(tmp[0]);
                                }
                                Err(err) => {
                                    inner.mark_error(err);
                                    continue 'poll;
                                }
                            }

                            if buf[buf_index] == END_OF_HEADER[end_index] {
                                end_index += 1;
                            } else if end_index > 0 {
                                end_index = 0;
                            }

                            if end_index == END_OF_HEADER.len() {
                                // we found the end of header sequence
                                break;
                            }
                            buf_index += 1;
                        }

                        // in theory we're now have a complete header ending \r\n\r\n
                        waker.clone().wake();

                        inner.state = State::RecvBody;

                        task_id_to_remove
                    } else {
                        inner.conn_waker = Some(cx.waker().clone());
                        return Poll::Pending;
                    };
                    inner.tasks.remove(to_remove);
                }
                State::RecvBody => {
                    let task = inner.tasks.get_recv_body(cur_seq);
                    let mut do_remove = false;
                    let to_remove = if let Some(Task::RecvBody(info, buf, end, waker)) = task {
                        let task_id_to_remove = info.1;
                        buf.resize(RECV_BODY_SIZE, 0);
                        let res = ready!(Pin::new(&mut self_.io).poll_read(cx, &mut buf[..]));
                        match res {
                            Ok(amount) => {
                                buf.resize(amount, 0);
                                if amount == 0 {
                                    do_remove = true;
                                    mem::replace(end, End(true));
                                }
                                waker.clone().wake();
                            }
                            Err(err) => {
                                inner.mark_error(err);
                                continue 'poll;
                            }
                        }

                        task_id_to_remove
                    } else {
                        inner.conn_waker = Some(cx.waker().clone());
                        return Poll::Pending;
                    };
                    if do_remove {
                        inner.tasks.remove(to_remove);
                    }
                }
                State::Closed => {
                    // The connection gets the original error.
                    let e = inner.error.take().unwrap();
                    let kind = e.kind();
                    let error = Error::Message(e.to_string());
                    let replacement = io::Error::new(kind, error);
                    inner.error = Some(replacement);
                    return Poll::Ready(Err(e));
                }
            }
        }
    }
}
