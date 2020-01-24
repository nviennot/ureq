use super::chunked::ChunkedDecoder;
use super::Error;
use super::RecvReader;

pub(crate) enum Limiter {
    ChunkedDecoder(ChunkedDecoder),
    ContenLength(ContentLength),
    UntilEnd(UntilEnd),
}

pub struct ContentLength {
    limit: u64,
    total: u64,
}

impl ContentLength {
    fn new(limit: u64) -> Self {
        ContentLength { limit, total: 0 }
    }
    async fn read_from(&mut self, recv: &mut RecvReader, buf: &mut [u8]) -> Result<usize, Error> {
        let left = (self.limit - self.total).min(usize::max_value() as u64) as usize;
        if left == 0 {
            return Ok(0);
        }
        let max = buf.len().min(left);
        let amount = recv.read(&mut buf[0..max]).await?;
        self.total += amount as u64;
        Ok(amount)
    }
}

pub struct UntilEnd;

impl UntilEnd {
    async fn read_from(&mut self, recv: &mut RecvReader, buf: &mut [u8]) -> Result<usize, Error> {
        Ok(recv.read(&mut buf[..]).await?)
    }
}

impl Limiter {
    pub fn from_response(res: &http::Response<()>) -> Self {
        let transfer_enc_chunk = res
            .headers()
            .get("transfer-encoding")
            .map(|h| h == "chunked")
            .unwrap_or(false);

        let content_length = res
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok().and_then(|c| c.parse::<u64>().ok()));

        let use_chunked = transfer_enc_chunk || content_length.is_none();

        if use_chunked {
            Limiter::ChunkedDecoder(ChunkedDecoder::new())
        } else if let Some(size) = content_length {
            Limiter::ContenLength(ContentLength::new(size))
        } else {
            Limiter::UntilEnd(UntilEnd)
        }
    }

    pub fn is_reusable_conn(&self) -> bool {
        // limiters read to stream end can't reuse connection.
        if let Limiter::UntilEnd(_) = self {
            return false;
        }
        true
    }

    pub async fn read_from(
        &mut self,
        recv: &mut RecvReader,
        buf: &mut [u8],
    ) -> Result<usize, Error> {
        match self {
            Limiter::ChunkedDecoder(v) => v.read_chunk(recv, buf).await,
            Limiter::ContenLength(v) => v.read_from(recv, buf).await,
            Limiter::UntilEnd(v) => v.read_from(recv, buf).await,
        }
    }
}
