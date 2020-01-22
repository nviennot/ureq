#![warn(clippy::all)]

//! What are we doing?
//!
//! * Parse URL (http)
//! * Set request headers (http)
//!   * Username/password
//! * Provide a request body (http)
//! * Resolve DNS (dns-lookup)
//! * Provide timeout for entire request.
//! * Connect socket … or is this API surface?
//! * Wrap socket in SSL (tls-api)
//! * Talk http (httparse)
//! * Talk http2 (h2?)
//! * Body data transformations
//!   * chunked encoding (my own)
//!   * x-www-form-urlencoded (write it?)
//!   * form-data (multipart) (write it?)
//! * Provide retry logic
//! * Connection pooling
//! * Cookie state in connection (cookie)
//! * HTTP Proxy
//!

#[macro_use]
extern crate log;

mod async_impl;
mod body;
mod chunked;
mod conn;
mod conn_http11;
mod conn_http2;
mod dlog;
mod either;
mod error;
mod http11;
mod limit;
mod peek;
mod proto;
mod req_ext;
mod tls;
mod tls_api_pass;
mod tokio;
mod uri;

pub use http;

pub use crate::error::Error;
pub(crate) use futures_io::{AsyncRead, AsyncWrite};
pub(crate) use futures_util::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) const PARSE_BUF_SIZE: usize = 16_384;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl Stream for Box<dyn Stream> {}

use crate::async_impl::AsyncImpl;
pub use crate::body::Body;
pub use crate::conn::Connection;
use crate::conn::ProtocolImpl;
use crate::either::Either;
use crate::peek::Peekable;
use crate::proto::Protocol;
pub use crate::req_ext::{RequestBuilderExt, RequestExt};
use crate::tokio::to_tokio;

pub use crate::tls::wrap_tls;

use tls_api::TlsConnector;

pub async fn connect<Tls: TlsConnector>(uri: &http::Uri) -> Result<Connection, Error> {
    crate::dlog::set_logger();
    let hostport = crate::uri::HostPort::from_uri(uri)?;
    // "host:port"
    let addr = hostport.to_string();

    let (stream, alpn_proto) = {
        // "raw" tcp
        let tcp = AsyncImpl::connect_tcp(&addr).await?;

        if hostport.is_tls() {
            // wrap in tls
            let (tls, proto) = wrap_tls::<Tls, _>(tcp, hostport.host()).await?;
            (Either::A(tls), proto)
        } else {
            // use tcp
            (Either::B(tcp), Protocol::Unknown)
        }
    };

    Ok(open_stream(stream, alpn_proto).await?)
}

pub fn connect_sync<Tls: TlsConnector>(uri: &http::Uri) -> Result<Connection, Error> {
    let fut = connect::<Tls>(uri);
    Ok(AsyncImpl::run_until(fut)?)
}

pub async fn open_stream(stream: impl Stream, proto: Protocol) -> Result<Connection, Error> {
    if proto == Protocol::Http2 {
        let (h2, h2conn) = h2::client::handshake(to_tokio(stream)).await?;
        // drives the connection independently of the h2 api surface.
        AsyncImpl::spawn(async {
            if let Err(err) = h2conn.await {
                // this is expected to happen when the connection disconnects
                trace!("Error in connection: {:?}", err);
            }
        });
        Ok(Connection::new(ProtocolImpl::Http2(h2)))
    } else {
        let boxed: Box<dyn Stream> = Box::new(stream);
        let peekable = Peekable::new(boxed, crate::PARSE_BUF_SIZE);
        Ok(Connection::new(ProtocolImpl::Http11(peekable)))
    }
}

pub fn open_stream_sync(stream: impl Stream, proto: Protocol) -> Result<Connection, Error> {
    let fut = open_stream(stream, proto);
    Ok(AsyncImpl::run_until(fut)?)
}

#[cfg(test)]
mod test {
    use super::tls_api_pass::TlsConnector as PassTlsConnector;
    use super::*;
    use tls_api_rustls::TlsConnector as RustlsTlsConnector;

    #[test]
    fn test_add_tls() -> Result<(), Error> {
        let req = http::Request::builder()
            .uri("https://www.google.com/")
            .body(Body::empty())
            .expect("Build");
        let conn = connect_sync::<RustlsTlsConnector>(req.uri())?;
        let res = conn.send_request_sync(req)?;
        let (_, mut body) = res.into_parts();
        let body_s = body.as_string_sync(1024 * 1024)?;
        println!("{}", body_s);
        Ok(())
    }

    #[test]
    fn test_add_no_tls() -> Result<(), Error> {
        let req = http::Request::builder()
            .uri("http://www.google.com/")
            .body(Body::empty())
            .expect("Build");
        let conn = connect_sync::<PassTlsConnector>(req.uri())?;
        let res = conn.send_request_sync(req)?;
        let (_, mut body) = res.into_parts();
        let body_s = body.as_string_sync(1024 * 1024)?;
        println!("{}", body_s);
        Ok(())
    }
}
