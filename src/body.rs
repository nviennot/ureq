use crate::stream::Stream;
use std::io::{copy, empty, Cursor, Read, Write, Result as IoResult};

#[cfg(feature = "charset")]
use crate::response::DEFAULT_CHARACTER_SET;
#[cfg(feature = "charset")]
use encoding::label::encoding_from_whatwg_label;
#[cfg(feature = "charset")]
use encoding::EncoderTrap;

#[cfg(feature = "json")]
use super::SerdeValue;
#[cfg(feature = "json")]
use serde_json;

/// The different kinds of bodies to send.
///
/// *Internal API*
pub(crate) enum Payload {
    Empty,
    Text(String, String),
    #[cfg(feature = "json")]
    JSON(SerdeValue),
    Reader(Box<dyn Read + 'static>),
    Bytes(Vec<u8>),
}

impl ::std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        match self {
            Payload::Empty => write!(f, "Empty"),
            Payload::Text(t, _) => write!(f, "{}", t),
            #[cfg(feature = "json")]
            Payload::JSON(_) => write!(f, "JSON"),
            Payload::Reader(_) => write!(f, "Reader"),
            Payload::Bytes(v) => write!(f, "{:?}", v),
        }
    }
}

impl Default for Payload {
    fn default() -> Payload {
        Payload::Empty
    }
}

/// Payloads are turned into this type where we can hold both a size and the reader.
///
/// *Internal API*
pub(crate) struct SizedReader {
    pub size: Option<usize>,
    pub reader: Box<dyn Read + 'static>,
}

impl ::std::fmt::Debug for SizedReader {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        write!(f, "SizedReader[size={:?},reader]", self.size)
    }
}

impl SizedReader {
    fn new(size: Option<usize>, reader: Box<dyn Read + 'static>) -> Self {
        SizedReader { size, reader }
    }
}

impl Payload {
    pub fn into_read(self) -> SizedReader {
        match self {
            Payload::Empty => SizedReader::new(None, Box::new(empty())),
            Payload::Text(text, _charset) => {
                #[cfg(feature = "charset")]
                let bytes = {
                    let encoding = encoding_from_whatwg_label(&_charset)
                        .or_else(|| encoding_from_whatwg_label(DEFAULT_CHARACTER_SET))
                        .unwrap();
                    encoding.encode(&text, EncoderTrap::Replace).unwrap()
                };
                #[cfg(not(feature = "charset"))]
                let bytes = text.into_bytes();
                let len = bytes.len();
                let cursor = Cursor::new(bytes);
                SizedReader::new(Some(len), Box::new(cursor))
            }
            #[cfg(feature = "json")]
            Payload::JSON(v) => {
                let bytes = serde_json::to_vec(&v).expect("Bad JSON in payload");
                let len = bytes.len();
                let cursor = Cursor::new(bytes);
                SizedReader::new(Some(len), Box::new(cursor))
            }
            Payload::Reader(read) => SizedReader::new(None, read),
            Payload::Bytes(bytes) => {
                let len = bytes.len();
                let cursor = Cursor::new(bytes);
                SizedReader::new(Some(len), Box::new(cursor))
            }
        }
    }
}

fn copy_chunked<R: ?Sized, W: ?Sized>(
    reader: &mut R,
    writer: &mut W,
    buf_size: usize
) -> IoResult<u64>
where
    R: Read,
    W: Write,
{
    // This version improves over chunked_transfer's Encoder + io::copy with the following
    // performance optimizations:
    // 1) The buffer used to do the transfer is configurable, which is useful when transfering
    //    large files. This reduces the overhead of going in the kernel. Small files don't
    //    benefit that much as the overhead of getting the initial page faults is significant.
    // 2) chunked_transfer's Encoder issues 4 separate writes per chunk. This can be costly
    //    in terms of overhead.Here, we do a single write per chunk, without using iovecs
    //    nor copying buffers.
    let header_reserve_len = 12;
    let footer = b"\r\n";
    let max_payload_size = buf_size - header_reserve_len - footer.len();

    // The chunk layout is:
    // header:header_reserve_len | payload:max_payload_size | footer:footer_len
    let mut chunk = Vec::with_capacity(buf_size);
    let mut written = 0;
    loop {
        // We first read the payload
        chunk.resize(header_reserve_len, 0);
        let len = reader.take(max_payload_size as u64).read_to_end(&mut chunk)?;

        // Then write the header
        let header_str = format!("{:x}\r\n", len);
        let header = header_str.as_bytes();
        assert!(header.len() <= header_reserve_len);
        let start_index = header_reserve_len - header.len();
        (&mut chunk[start_index..]).write(&header).unwrap();

        // And add the footer
        chunk.extend_from_slice(footer);

        writer.write_all(&chunk[start_index..])?;
        written += len as u64;

        if len == 0 {
            return Ok(written);
        }
    }
}

#[cfg(test)]
mod test {
    use super::copy_chunked;
    use std::io;
    use std::str::from_utf8;

    #[test]
    fn test_copy_chunked() {
        let mut source = io::Cursor::new("hello world".to_string().into_bytes());
        let mut dest: Vec<u8> = vec![];
        copy_chunked(&mut source, &mut dest, 12+5+2).unwrap();
        let output = from_utf8(&dest).unwrap();

        assert_eq!(output, "5\r\nhello\r\n5\r\n worl\r\n1\r\nd\r\n0\r\n\r\n");
    }
}

/// Helper to send a body, either as chunked or not.
pub(crate) fn send_body(
    mut body: SizedReader,
    do_chunk: bool,
    stream: &mut Stream,
    buf_size: Option<usize>,
) -> IoResult<u64> {
    let n = if do_chunk {
        let buf_size = buf_size.unwrap_or(8*1024);
        copy_chunked(&mut body.reader, stream, buf_size)?
    } else {
        copy(&mut body.reader, stream)?
    };

    Ok(n)
}
