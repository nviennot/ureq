use crate::chunked::ChunkedDecoder;
use crate::conn::ProtocolImpl;
use crate::h1::SendRequest as H1SendRequest;
use crate::limit::LimitRead;
use crate::peek::Peekable;
use crate::Connection;
use crate::Error;
use crate::Stream;
use crate::{AsyncRead, AsyncReadExt};
use bytes::Bytes;
use h2::client::SendRequest as H2SendRequest;
use h2::RecvStream;
use std::io;

const BUF_SIZE: usize = 16_384;

pub struct Body {
    inner: BodyImpl,
    leftovers: Option<Bytes>,
    is_finished: bool,
}

pub enum BodyImpl {
    RequestEmpty,
    RequestAsyncRead(Box<dyn AsyncRead + Unpin + Send>),
    RequestRead(Box<dyn io::Read + Send>),
    Http1(H1SendRequest),
    Http11Chunked(ChunkedDecoder),
    Http11Limited(LimitRead),
    Http11Unlimited(Peekable<Box<dyn Stream>>),
    Http2(RecvStream, H2SendRequest<Bytes>),
}

impl Body {
    pub fn empty() -> Self {
        Self::new(BodyImpl::RequestEmpty)
    }
    pub fn from_async_read<R: AsyncRead + Unpin + Send + 'static>(reader: R) -> Self {
        Self::new(BodyImpl::RequestAsyncRead(Box::new(reader)))
    }
    pub fn from_sync_read<R: io::Read + Send + 'static>(reader: R) -> Self {
        Self::new(BodyImpl::RequestRead(Box::new(reader)))
    }
    pub(crate) fn new(inner: BodyImpl) -> Self {
        Body {
            inner,
            leftovers: None,
            is_finished: false,
        }
    }

    pub async fn into_connection(mut self) -> Result<Connection, Error> {
        // http11 reuses the same connection, and we can't leave the body
        // half way through read.
        if self.is_http11() && !self.is_finished {
            self.read_to_end().await?;
        }

        let conn = match self.inner {
            BodyImpl::Http11Chunked(decoder) => {
                Connection::new(ProtocolImpl::Http11(decoder.into_inner()))
            }
            BodyImpl::Http11Limited(limiter) => {
                Connection::new(ProtocolImpl::Http11(limiter.into_inner()))
            }
            BodyImpl::Http11Unlimited(_) => Connection::new(ProtocolImpl::Unusable {
                reason: "Previous response did not contain a content-length header.",
            }),
            BodyImpl::Http2(_, h2) => Connection::new(ProtocolImpl::Http2(h2)),
            _ => return Err(Error::Static("Can't do into_connection() on request body")),
        };

        Ok(conn)
    }

    fn is_http11(&self) -> bool {
        match &self.inner {
            BodyImpl::Http11Chunked(_) => true,
            BodyImpl::Http11Limited(_) => true,
            BodyImpl::Http11Unlimited(_) => true,
            _ => false,
        }
    }

    async fn read_to_end(&mut self) -> Result<(), Error> {
        let mut buf = vec![0_u8; BUF_SIZE];
        loop {
            let read = self.read(&mut buf).await?;
            if read == 0 {
                break;
            }
        }
        Ok(())
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        assert!(!buf.is_empty(), "Can't read with an empty buf");
        if self.is_finished || buf.is_empty() {
            return Ok(0);
        }
        // h2 streams might have leftovers to use up before reading any more.
        if let Some(data) = self.leftovers.take() {
            let amount = self.bytes_to_buf(data, buf);
            return Ok(amount);
        }
        let read = match &mut self.inner {
            BodyImpl::RequestEmpty => 0,
            BodyImpl::RequestAsyncRead(reader) => reader.read(buf).await?,
            BodyImpl::RequestRead(reader) => reader.read(buf)?,
            BodyImpl::Http1(_) => 0,
            BodyImpl::Http11Chunked(decoder) => decoder.read_chunk(buf).await?,
            BodyImpl::Http11Limited(limiter) => limiter.read(buf).await?,
            BodyImpl::Http11Unlimited(stream) => stream.read(buf).await?,
            BodyImpl::Http2(recv, _) => {
                if let Some(data) = recv.data().await {
                    let data = data?;
                    self.bytes_to_buf(data, buf)
                } else {
                    0
                }
            }
        };
        if read == 0 {
            self.is_finished = true;
        }
        Ok(read)
    }

    // helper to shuffle Bytes into a &[u8] and handle the remains.
    fn bytes_to_buf(&mut self, mut data: Bytes, buf: &mut [u8]) -> usize {
        let max = data.len().min(buf.len());
        (&mut buf[0..max]).copy_from_slice(&data[0..max]);
        let remain = if max < data.len() {
            Some(data.split_off(max))
        } else {
            None
        };
        self.leftovers = remain;
        max
    }

    pub async fn as_vec(&mut self, max: usize) -> Result<Vec<u8>, Error> {
        let mut vec = Vec::new();
        let mut total_read = 0;
        loop {
            let remaining_reserved = vec.len() - total_read;
            if remaining_reserved < 128 {
                if vec.len() == max {
                    // we can't grow vec any more
                    return Err(Error::Message(format!("Reached max to read: {}", max)));
                }
                // reserve more space, but only up to max
                let reserve_to_size = (vec.len() + BUF_SIZE).min(max);
                vec.resize(reserve_to_size, 0);
            }
            let amount = self.read(&mut vec[total_read..]).await?;
            if amount == 0 {
                break;
            }
            total_read += amount;
        }
        // size down if we reserved too much
        vec.resize(total_read, 0);
        Ok(vec)
    }

    pub async fn as_string(&mut self, max: usize) -> Result<String, Error> {
        let bytes = self.as_vec(max).await?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Body::empty()
    }
}

impl<'a> From<&'a str> for Body {
    fn from(s: &'a str) -> Self {
        s.to_owned().into()
    }
}

impl<'a> From<&'a String> for Body {
    fn from(s: &'a String) -> Self {
        s.clone().into()
    }
}

impl From<String> for Body {
    fn from(s: String) -> Self {
        let bytes = s.into_bytes();
        bytes.into()
    }
}

impl<'a> From<&'a [u8]> for Body {
    fn from(bytes: &'a [u8]) -> Self {
        bytes.to_vec().into()
    }
}

impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Self {
        let cursor = io::Cursor::new(bytes);
        Body::from_sync_read(cursor)
    }
}
