use crate::peek::Peekable;
use crate::Error;
use crate::Stream;
use crate::{AsyncReadExt, AsyncWriteExt};
use std::io;

/// Decode AsyncRead as transfer-encoding chunked.
pub struct ChunkedDecoder {
    stream: Peekable<Box<dyn Stream>>,
    amount_left: usize,
    pub(crate) is_finished: bool,
}

impl ChunkedDecoder {
    pub fn new(stream: Peekable<Box<dyn Stream>>) -> Self {
        ChunkedDecoder {
            stream,
            amount_left: 0,
            is_finished: false,
        }
    }

    pub fn into_inner(self) -> Peekable<Box<dyn Stream>> {
        self.stream
    }

    pub async fn read_chunk(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.is_finished {
            return Ok(0);
        }
        if self.amount_left == 0 {
            let chunk_size = self.read_chunk_size(buf).await?;
            if chunk_size == 0 {
                self.is_finished = true;
                return Ok(0);
            }
            self.amount_left = chunk_size;
        }
        let to_read = self.amount_left.min(buf.len());
        self.amount_left -= to_read;
        self.stream.read_exact(&mut buf[0..to_read]).await?;
        if self.amount_left == 0 {
            // skip \r\n after the chunk
            self.skip_until_lf().await?;
        }
        Ok(to_read)
    }

    // 3\r\nhel\r\nb\r\nlo world!!!\r\n0\r\n\r\n
    async fn read_chunk_size(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // read until we get a non-numeric character. this could be
        // either \r or maybe a ; if we are using "extensions"
        let mut pos = 0;
        loop {
            self.stream.read_exact(&mut buf[pos..=pos]).await?;
            let c: char = buf[pos].into();
            // keep reading until we get ; or \r
            if c == ';' || c == '\r' {
                break;
            }
            pos += 1;
        }

        self.skip_until_lf().await?;

        // no length, no number to parse.
        if buf.is_empty() {
            return Ok(0);
        }

        // parse the read numbers as a chunk size.
        let chunk_size_s = String::from_utf8_lossy(&buf[0..pos]);
        let chunk_size = usize::from_str_radix(chunk_size_s.trim(), 16)
            .ok()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Not a number in chunk size")
            })?;

        Ok(chunk_size)
    }

    // skip until we get a \n
    async fn skip_until_lf(&mut self) -> io::Result<()> {
        // skip until we get a \n
        let mut one = [0_u8; 1];
        loop {
            self.stream.read_exact(&mut one[..]).await?;
            if one[0] == b'\n' {
                break;
            }
        }
        Ok(())
    }
}

/// Transfer encoding chunked to an AsyncWrite
pub struct ChunkedEncoder;

impl ChunkedEncoder {
    pub async fn send_chunk(buf: &[u8], stream: &mut impl Stream) -> Result<(), Error> {
        let header = format!("{}\r\n", buf.len()).into_bytes();
        stream.write_all(&header[..]).await?;
        stream.write_all(&buf[..]).await?;
        const CRLF: &[u8] = b"\r\n";
        stream.write_all(CRLF).await?;
        Ok(())
    }
    pub async fn send_finish(stream: &mut impl Stream) -> Result<(), Error> {
        const END: &[u8] = b"0\r\n\r\n";
        stream.write_all(END).await?;
        Ok(())
    }
}
