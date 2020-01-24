mod chunked;
mod error;
mod http11;
mod limit;
mod task;

pub use error::Error;
pub(crate) use futures_io::{AsyncRead, AsyncWrite};
use futures_util::future::poll_fn;
pub(crate) use futures_util::io::AsyncReadExt;
use futures_util::ready;
use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use task::{End, RecvBody, RecvRes, SendBody, SendReq, Seq, Task, Tasks};

const RECV_BODY_SIZE: usize = 16_384;

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
            let task = SendReq::from_req(seq, req, end)?;
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
        if let Some(task) = inner.tasks.get_recv_res(self.seq) {
            let req = task.try_parse()?;
            if let Some(req) = req {
                let recv_stream = RecvStream::new(self.inner.clone(), self.seq);
                let (parts, _) = req.into_parts();
                Poll::Ready(Ok(http::Response::from_parts(parts, recv_stream)))
            } else {
                mem::replace(&mut task.waker, cx.waker().clone());
                Poll::Pending
            }
        } else {
            let task = RecvRes::new(self.seq, cx.waker().clone());
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
        if let Some(task) = inner.tasks.get_send_body(self.seq) {
            task.send_waker.replace(cx.waker().clone());
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
        let task = SendBody::new(self.seq, data.to_owned(), end);
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
        if let Some(task) = inner.tasks.get_recv_body(self.seq) {
            mem::replace(&mut task.waker, cx.waker().clone());
            let buf = &mut task.buf;
            if buf.is_empty() && !*task.end {
                Poll::Pending
            } else {
                let max = buf.len().min(out.len());
                (&mut out[0..max]).copy_from_slice(&buf[0..max]);
                let rest = buf.split_off(max);
                mem::replace(buf, rest);
                Poll::Ready(Ok(max))
            }
        } else {
            let task = RecvBody::new(self.seq, cx.waker().clone());
            inner.enqueue(task);
            Poll::Pending
        }
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        Ok(poll_fn(|cx| self.poll_for_content(cx, buf)).await?)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum State {
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

    fn enqueue<T: Into<Task>>(&mut self, task: T) {
        self.tasks.push(task.into());
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
        loop {
            let cur_seq = Seq(inner.cur_seq);
            let mut state = inner.state; // copy to appease borrow checker

            if state == State::Closed {
                if let Some(e) = inner.error.as_mut() {
                    let repl = io::Error::new(e.kind(), Error::Message(e.to_string()));
                    let orig = mem::replace(e, repl);
                    return Poll::Ready(Err(orig));
                } else {
                    return Poll::Ready(Ok(()));
                }
            }

            if let Some(task) = inner.tasks.task_for_state(cur_seq, state) {
                let task_id = task.info().task_id;

                let complete = match ready!(task.poll_connection(cx, &mut self_.io, &mut state)) {
                    Ok(v) => {
                        // update state back to inner
                        inner.state = state;
                        v
                    }
                    Err(err) => {
                        inner.mark_error(err);
                        true // this task is complete
                    }
                };

                if complete {
                    inner.tasks.remove(task_id);
                }
            } else {
                inner.conn_waker = Some(cx.waker().clone());
                break Poll::Pending;
            }
        }
    }
}

trait ConnectionPoll {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin;
}

impl ConnectionPoll for SendReq {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            let amount = ready!(Pin::new(&mut *io).poll_write(cx, &self.req[..]))?;
            if amount < self.req.len() {
                let rest = self.req.split_off(amount);
                mem::replace(&mut self.req, rest);
                continue;
            }
            break;
        }
        if *self.end {
            *state = State::Waiting;
        } else {
            *state = State::SendBody;
        }
        Poll::Ready(Ok(true))
    }
}

impl ConnectionPoll for SendBody {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            let amount = ready!(Pin::new(&mut *io).poll_write(cx, &self.body[..]))?;
            if amount < self.body.len() {
                let rest = self.body.split_off(amount);
                mem::replace(&mut self.body, rest);
                continue;
            }
            break;
        }
        // entire current send_body was sent, waker is for a
        // someone potentially waiting to send more.
        if let Some(waker) = self.send_waker.take() {
            waker.wake();
        }

        let task_complete = if *self.end {
            *state = State::Waiting;
            true
        } else {
            false
        };

        Poll::Ready(Ok(task_complete))
    }
}

impl ConnectionPoll for RecvRes {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        const END_OF_HEADER: &[u8] = &[b'\r', b'\n', b'\r', b'\n'];
        let mut end_index = 0;
        let mut buf_index = 0;
        let mut one = [0_u8; 1];
        loop {
            if buf_index == self.buf.len() {
                // read one more char
                let amount = ready!(Pin::new(&mut &mut *io).poll_read(cx, &mut one[..]))?;
                if amount == 0 {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "EOF before complete http11 header",
                    )));
                }
                self.buf.push(one[0]);
            }

            if self.buf[buf_index] == END_OF_HEADER[end_index] {
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

        *state = State::RecvBody;

        // in theory we're now have a complete header ending \r\n\r\n
        self.waker.clone().wake();

        // task is only complete when waker reads response
        Poll::Ready(Ok(false))
    }
}

impl ConnectionPoll for RecvBody {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        self.buf.resize(RECV_BODY_SIZE, 0);
        let amount = ready!(Pin::new(&mut *io).poll_read(cx, &mut self.buf[..]))?;
        self.buf.resize(amount, 0);

        if amount == 0 {
            mem::replace(&mut self.end, End(true));
            *state = State::Ready;
        }

        self.waker.clone().wake();

        // task is complete when waker reads content
        Poll::Ready(Ok(false))
    }
}

impl ConnectionPoll for Task {
    fn poll_connection<S>(
        &mut self,
        cx: &mut Context,
        io: &mut S,
        state: &mut State,
    ) -> Poll<io::Result<bool>>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        match self {
            Task::SendReq(t) => t.poll_connection(cx, io, state),
            Task::SendBody(t) => t.poll_connection(cx, io, state),
            Task::RecvRes(t) => t.poll_connection(cx, io, state),
            Task::RecvBody(t) => t.poll_connection(cx, io, state),
        }
    }
}
