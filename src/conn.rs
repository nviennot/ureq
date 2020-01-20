use crate::conn_http11::{read_response_http11, response_body_http11, send_request_http11};
use crate::conn_http2::send_request_http2;
use crate::peek::Peekable;
use crate::Body;
use crate::Error;
use crate::Stream;
use bytes::Bytes;
use h2::client::SendRequest;
use std::fmt;

pub enum ProtocolImpl {
    Unusable { reason: &'static str },
    Http11(Peekable<Box<dyn Stream>>),
    Http2(SendRequest<Bytes>),
}

impl fmt::Display for ProtocolImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolImpl::Unusable { reason } => write!(f, "Unusable: {}", reason),
            ProtocolImpl::Http11(_) => write!(f, "Http11"),
            ProtocolImpl::Http2(_) => write!(f, "Http2"),
        }
    }
}

pub struct Connection {
    p: ProtocolImpl,
}

impl Connection {
    pub(crate) fn new(p: ProtocolImpl) -> Self {
        Connection { p }
    }

    pub fn is_usable(&self) -> bool {
        if let ProtocolImpl::Unusable { .. } = &self.p {
            return false;
        }
        true
    }

    pub fn maybe_clone(&self) -> Option<Connection> {
        if let ProtocolImpl::Http2(send_req) = &self.p {
            return Some(Connection::new(ProtocolImpl::Http2(send_req.clone())));
        }
        None
    }

    pub async fn send_request(
        self,
        req: http::Request<Body>,
    ) -> Result<http::Response<Body>, Error> {
        match self.p {
            ProtocolImpl::Http11(mut stream) => {
                let mut buf = vec![0; crate::PARSE_BUF_SIZE];
                send_request_http11(&mut stream, req, &mut buf).await?;
                let resp = read_response_http11(&mut stream, &mut buf).await?;
                Ok(response_body_http11(resp, stream).await?)
            }
            ProtocolImpl::Http2(send_req) => Ok(send_request_http2(send_req, req).await?),
            ProtocolImpl::Unusable { reason } => Err(Error::Message(format!(
                "Connection is unusable: {}",
                reason
            ))),
        }
    }
}
