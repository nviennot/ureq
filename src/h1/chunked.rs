use super::Error;
use super::RecvReader;
use std::io;

/// Decode AsyncRead as transfer-encoding chunked.
pub(crate) struct ChunkedDecoder {
    amount_left: usize,
    pub(crate) is_finished: bool,
}

impl ChunkedDecoder {
    pub fn new() -> Self {
        ChunkedDecoder {
            amount_left: 0,
            is_finished: false,
        }
    }

    pub async fn read_chunk(
        &mut self,
        recv: &mut RecvReader,
        buf: &mut [u8],
    ) -> Result<usize, Error> {
        if self.is_finished {
            return Ok(0);
        }
        if self.amount_left == 0 {
            let chunk_size = self.read_chunk_size(recv, buf).await?;
            trace!("Chunk size: {}", chunk_size);
            if chunk_size == 0 {
                self.is_finished = true;
                return Ok(0);
            }
            self.amount_left = chunk_size;
        }
        let to_read = self.amount_left.min(buf.len());
        let amount_read = recv.read(&mut buf[0..to_read]).await?;
        self.amount_left -= amount_read;
        if self.amount_left == 0 {
            // skip \r\n after the chunk
            self.skip_until_lf(recv).await?;
        }
        Ok(to_read)
    }

    // 3\r\nhel\r\nb\r\nlo world!!!\r\n0\r\n\r\n
    async fn read_chunk_size(
        &mut self,
        recv: &mut RecvReader,
        buf: &mut [u8],
    ) -> Result<usize, Error> {
        // read until we get a non-numeric character. this could be
        // either \r or maybe a ; if we are using "extensions"
        let mut pos = 0;
        loop {
            recv.read(&mut buf[pos..=pos]).await?;
            let c: char = buf[pos].into();
            // keep reading until we get ; or \r
            if c == ';' || c == '\r' {
                break;
            }
            if c == '0'
                || c == '1'
                || c == '2'
                || c == '3'
                || c == '4'
                || c == '5'
                || c == '6'
                || c == '7'
                || c == '8'
                || c == '9'
                || c == 'a'
                || c == 'b'
                || c == 'c'
                || c == 'd'
                || c == 'e'
                || c == 'f'
            {
                // good
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unexpected char in chunk size: {:?}", c),
                )
                .into());
            }
            pos += 1;
            if pos > 10 {
                // something is wrong.
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Too many chars in number",
                )
                .into());
            }
        }

        self.skip_until_lf(recv).await?;

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
    async fn skip_until_lf(&mut self, recv: &mut RecvReader) -> Result<(), Error> {
        // skip until we get a \n
        let mut one = [0_u8; 1];
        loop {
            recv.read(&mut one[..]).await?;
            if one[0] == b'\n' {
                break;
            }
        }
        Ok(())
    }
}

// /// Transfer encoding chunked to an AsyncWrite
// pub struct ChunkedEncoder;

// impl ChunkedEncoder {
//     pub async fn send_chunk(buf: &[u8], stream: &mut impl Stream) -> Result<(), Error> {
//         let header = format!("{}\r\n", buf.len()).into_bytes();
//         stream.write_all(&header[..]).await?;
//         stream.write_all(&buf[..]).await?;
//         const CRLF: &[u8] = b"\r\n";
//         stream.write_all(CRLF).await?;
//         Ok(())
//     }
//     pub async fn send_finish(stream: &mut impl Stream) -> Result<(), Error> {
//         const END: &[u8] = b"0\r\n\r\n";
//         stream.write_all(END).await?;
//         Ok(())
//     }
// }
