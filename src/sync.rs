use crate::async_impl::AsyncImpl;
use crate::proto::Protocol;
use crate::Body;
use crate::Error;
use crate::Stream;

pub struct Connection(crate::Connection);

impl Connection {
    pub fn is_usable(&self) -> bool {
        self.0.is_usable()
    }
    pub fn maybe_clone(&self) -> Option<Connection> {
        self.0.maybe_clone().map(Connection)
    }
    pub fn send_request(self, req: http::Request<Body>) -> Result<http::Response<Body>, Error> {
        let (req_parts, sync_req_body) = req.into_parts();
        let async_req = http::Request::from_parts(req_parts, sync_req_body);
        let fut = self.0.send_request(async_req);
        let (res_parts, res_body) = AsyncImpl::run_until(fut)?.into_parts();
        Ok(http::Response::from_parts(res_parts, res_body))
    }
}

#[cfg(feature = "tls")]
pub use crate::tls::wrap_tls;

#[cfg(feature = "tls")]
use tls_api::TlsConnector;

#[cfg(feature = "tls")]
pub fn connect<Tls: TlsConnector, X>(req: &http::Request<X>) -> Result<Connection, Error> {
    crate::log::set_logger();
    let fut = crate::connect::<Tls, X>(req);
    Ok(Connection(AsyncImpl::run_until(fut)?))
}

#[cfg(feature = "tls")]
pub fn connect_uri<Tls: TlsConnector>(uri: &http::Uri) -> Result<Connection, Error> {
    let fut = crate::connect_uri::<Tls>(uri);
    Ok(Connection(AsyncImpl::run_until(fut)?))
}

#[cfg(not(feature = "tls"))]
pub fn connect_uri(uri: &http::Uri) -> Result<Connection, Error> {
    let fut = crate::connect_uri(uri);
    Ok(Connection(AsyncImpl::run_until(fut)?))
}

pub fn connect_stream(stream: impl Stream, proto: Protocol) -> Result<Connection, Error> {
    let fut = crate::connect_stream(stream, proto);
    Ok(Connection(AsyncImpl::run_until(fut)?))
}
