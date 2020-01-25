#![warn(clippy::all)]

#[macro_use]
extern crate log;

mod async_impl;
mod body;
mod conn;
mod conn_http1;
// mod conn_http11;
mod char_enc;
mod conn_http2;
mod dlog;
mod either;
mod error;
pub mod h1;
mod proto;
mod req_ext;
mod tls;
mod tls_pass;
mod tokio;
mod uri;

pub use crate::error::Error;
pub(crate) use futures_io::{AsyncBufRead, AsyncRead, AsyncWrite};
pub use http;

use crate::async_impl::exec::AsyncImpl;
pub use crate::body::Body;
pub use crate::conn::Connection;
use crate::conn::ProtocolImpl;
use crate::either::Either;
use crate::proto::Protocol;
pub use crate::req_ext::{RequestBuilderExt, RequestExt};
use crate::tls::wrap_tls;
use crate::tokio::to_tokio;
use std::future::Future;
use tls_api::TlsConnector;
pub use tls_pass::TlsConnector as PassTlsConnector;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl Stream for Box<dyn Stream> {}

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

    open_stream(stream, alpn_proto).await
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
        let (h1, h1conn) = h1::handshake(stream);
        // drives the connection independently of the h1 api surface
        AsyncImpl::spawn(async {
            if let Err(err) = h1conn.await {
                // this is expected to happen when the connection disconnects
                trace!("Error in connection: {:?}", err);
            }
        });
        Ok(Connection::new(ProtocolImpl::Http1(h1)))
    }
}

pub fn block_on<Ret, Fut: Future<Output = Ret>>(f: Fut) -> Ret {
    AsyncImpl::block_on(f)
}

#[cfg(test)]
mod test {
    use super::*;
    use futures_util::io::AsyncReadExt;
    use tls_api_rustls::TlsConnector as RustlsTlsConnector;

    #[test]
    fn test_tls() -> Result<(), Error> {
        let req = http::Request::builder()
            .uri("https://lookback.io/")
            .query("foo", "bar")
            .header("accept-encoding", "gzip")
            .body(Body::empty())
            .expect("Build");
        let body_s: Result<String, Error> = block_on(async {
            let conn = connect::<RustlsTlsConnector>(req.uri()).await?;
            let res = conn.send_request(req).await?;
            let (_, mut body) = res.into_parts();
            let mut buffer = String::with_capacity(4);
            body.read_to_string(&mut buffer).await?;
            Ok(buffer)
        });
        println!("{}", body_s?);
        Ok(())
    }

    #[test]
    fn test_no_tls() -> Result<(), Error> {
        let req = http::Request::builder()
            .uri("http://www.google.com/")
            // .header("accept-encoding", "gzip")
            .from_body(())
            .expect("Build");
        let body_s: Result<String, Error> = block_on(async {
            let conn = connect::<PassTlsConnector>(req.uri()).await?;
            let res = conn.send_request(req).await?;
            let (_, mut body) = res.into_parts();
            let mut buffer = String::with_capacity(4);
            body.read_to_string(&mut buffer).await?;
            Ok(buffer)
        });
        println!("{}", body_s?);
        Ok(())
    }
}
