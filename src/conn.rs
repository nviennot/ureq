use crate::conn_http1::send_request_http1;
use crate::conn_http2::send_request_http2;
use crate::h1::SendRequest as H1SendRequest;
use crate::req_ext::resolve_ureq_ext;
use crate::req_ext::RequestParams;
use crate::Body;
use crate::Error;
use bytes::Bytes;
use h2::client::SendRequest as H2SendRequest;
use once_cell::sync::Lazy;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

static ID_COUNTER: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

#[derive(Clone)]
pub enum ProtocolImpl {
    Http1(H1SendRequest),
    Http2(H2SendRequest<Bytes>),
}

impl fmt::Display for ProtocolImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolImpl::Http1(_) => write!(f, "Http1"),
            ProtocolImpl::Http2(_) => write!(f, "Http2"),
        }
    }
}

// #[derive(Clone)]
pub struct Connection {
    id: usize,
    addr: String,
    p: ProtocolImpl,
    unfinished_reqs: Arc<()>,
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Connection {}

impl Connection {
    pub(crate) fn new(addr: String, p: ProtocolImpl) -> Self {
        Connection {
            id: ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            addr,
            p,
            unfinished_reqs: Arc::new(()),
        }
    }

    pub(crate) fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn addr(&self) -> &str {
        &self.addr
    }

    pub(crate) fn is_http2(&self) -> bool {
        match self.p {
            ProtocolImpl::Http1(_) => false,
            ProtocolImpl::Http2(_) => true,
        }
    }

    pub(crate) fn unfinished_requests(&self) -> usize {
        Arc::strong_count(&self.unfinished_reqs) - 1 // -1 for self
    }

    pub async fn send_request(
        &mut self,
        req: http::Request<Body>,
    ) -> Result<http::Response<Body>, Error> {
        // up the arc-counter on unfinished reqs
        let unfin = self.unfinished_reqs.clone();

        let (mut parts, mut body) = req.into_parts();

        // apply ureq request builder extensions.
        if let Some(req_params) = resolve_ureq_ext(&mut parts) {
            parts.extensions.insert(req_params);
        } else {
            parts.extensions.insert(RequestParams::new());
        }

        let deadline = {
            // set req_start to be able to measure connection time
            let ext = parts.extensions.get_mut::<RequestParams>().unwrap();
            ext.req_start = Some(Instant::now());
            ext.deadline()
        };

        // resolve deferred body codecs now that we know the headers.
        body.configure(deadline, &parts.headers, false);

        let req = http::Request::from_parts(parts, body);

        trace!("{} {} {} {}", self.p, self.addr, req.method(), req.uri());

        match &mut self.p {
            ProtocolImpl::Http1(send_req) => {
                let s = send_req.clone();
                deadline.race(send_request_http1(s, req, unfin)).await
            }
            ProtocolImpl::Http2(send_req) => {
                let s = send_req.clone();
                deadline.race(send_request_http2(s, req, unfin)).await
            }
        }
    }
}
