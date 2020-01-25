use crate::conn_http1::send_request_http1;
use crate::conn_http2::send_request_http2;
use crate::h1::SendRequest as H1SendRequest;
use crate::req_ext::resolve_ureq_ext;
use crate::req_ext::RequestParams;
use crate::Body;
use crate::Error;
use bytes::Bytes;
use h2::client::SendRequest as H2SendRequest;
use std::fmt;
use std::time::Instant;

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

#[derive(Clone)]
pub struct Connection {
    p: ProtocolImpl,
}

impl Connection {
    pub(crate) fn new(p: ProtocolImpl) -> Self {
        Connection { p }
    }

    pub async fn send_request(
        self,
        req: http::Request<Body>,
    ) -> Result<http::Response<Body>, Error> {
        //
        let (mut parts, mut body) = req.into_parts();

        // apply ureq request builder extensions.
        if let Some(req_params) = resolve_ureq_ext(&mut parts) {
            parts.extensions.insert(req_params);
        } else {
            parts.extensions.insert(RequestParams::new());
        }

        {
            // set req_start to be able to measure connection time
            let ext = parts.extensions.get_mut::<RequestParams>().unwrap();
            ext.req_start = Some(Instant::now());
        }

        // resolve deferred body codecs now that we know the headers.
        body.configure(&parts.headers, false);

        let req = http::Request::from_parts(parts, body);

        trace!("{} {} {}", self.p, req.method(), req.uri());

        match self.p {
            ProtocolImpl::Http1(send_req) => send_request_http1(send_req, req).await,
            ProtocolImpl::Http2(send_req) => send_request_http2(send_req, req).await,
        }
    }
}
