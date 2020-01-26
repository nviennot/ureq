use crate::connect;
use crate::uri::UriExt;
use crate::Body;
use crate::Connection;
use crate::Error;
use std::marker::PhantomData;
use tls_api::TlsConnector;

#[derive(Default)]
pub struct Agent<Tls: TlsConnector> {
    connections: Vec<Connection>,
    _ph: PhantomData<Tls>,
}

impl<Tls: TlsConnector> Agent<Tls> {
    pub fn new() -> Self {
        Agent {
            connections: vec![],
            _ph: PhantomData,
        }
    }

    fn reuse_from_pool(&mut self, uri: &http::Uri) -> Result<Option<&mut Connection>, Error> {
        let hostport = uri.host_port()?;
        let addr = hostport.to_string();
        Ok(self
            .connections
            .iter_mut()
            // http2 multiplexes over the same connection, http1 needs to finish previous req
            .find(|c| c.addr() == addr && (c.is_http2() || c.unfinished_requests() == 0)))
    }

    async fn connect(&mut self, uri: &http::Uri) -> Result<&mut Connection, Error> {
        let conn = connect::<Tls>(uri).await?;
        self.connections.push(conn);
        let idx = self.connections.len() - 1;
        Ok(self.connections.get_mut(idx).unwrap())
    }

    pub async fn run(&mut self, req: http::Request<Body>) -> Result<http::Response<Body>, Error> {
        let conn = match self.reuse_from_pool(req.uri())? {
            Some(conn) => conn,
            None => self.connect(req.uri()).await?,
        };
        conn.send_request(req).await
    }
}
