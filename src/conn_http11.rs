use crate::body::{Body, BodyImpl};
use crate::chunked::{ChunkedDecoder, ChunkedEncoder};
use crate::http11::{try_parse_http11, write_http11_req};
use crate::limit::LimitRead;
use crate::peek::Peekable;
use crate::Error;
use crate::Stream;
use crate::{AsyncReadExt, AsyncWriteExt};

pub async fn send_request_http11(
    stream: &mut Peekable<Box<dyn Stream>>,
    req: http::Request<Body>,
    buf: &mut [u8],
) -> Result<(), Error> {
    // write request header
    let size = write_http11_req(&req, buf)?;
    stream.write_all(&buf[0..size]).await?;

    // send request body
    send_req_body(stream, req, &mut buf[..]).await?;

    Ok(())
}

async fn send_req_body(
    stream: &mut Peekable<Box<dyn Stream>>,
    req: http::Request<Body>,
    buf: &mut [u8],
) -> Result<(), Error> {
    let transfer_enc_chunk = req
        .headers()
        .get("transfer-encoding")
        .map(|h| h == "chunked")
        .unwrap_or(false);

    let content_size = req
        .headers()
        .get("content-size")
        .and_then(|h| h.to_str().ok().and_then(|c| c.parse::<usize>().ok()));

    let use_chunked = transfer_enc_chunk || content_size.is_none();

    // TODO fix gzip/deflate/etc

    // push body
    let (_, mut body) = req.into_parts();
    if use_chunked {
        const MAX_CHUNK_SIZE: usize = 1024;
        assert!(MAX_CHUNK_SIZE < buf.len());
        // chunked transfer encoding
        loop {
            let amount = body.read(&mut buf[..MAX_CHUNK_SIZE]).await?;
            if amount == 0 {
                break;
            }
            ChunkedEncoder::send_chunk(&buf[0..amount], stream).await?;
        }
        ChunkedEncoder::send_finish(stream).await?;
    } else {
        // no transfer encoding
        loop {
            let amount = body.read(&mut buf[..]).await?;
            if amount == 0 {
                break;
            }
            stream.write_all(&buf[0..amount]).await?;
        }
    }

    Ok(())
}

pub async fn read_response_http11(
    stream: &mut Peekable<Box<dyn Stream>>,
    buf: &mut [u8],
) -> Result<http::Response<()>, Error> {
    // try peek/parsing response
    let (res, parsed_len) = loop {
        let amount = stream.peek(&mut buf[..]).await?;
        if amount == 0 {
            break None;
        }
        let res = try_parse_http11(&buf[0..amount])?;
        if res.is_some() {
            break res;
        }
        if amount == buf.len() {
            // we peaked the entire buf, and it didn't help.
            break None;
        }
    }
    .ok_or_else(|| Error::Static("Failed to parse HTTP1.1 response"))?;

    // skip parsed_len that was used for the request header
    assert!(parsed_len < buf.len());
    stream.read_exact(&mut buf[0..parsed_len]).await?;

    Ok(res)
}

pub async fn response_body_http11(
    res: http::Response<()>,
    stream: Peekable<Box<dyn Stream>>,
) -> Result<http::Response<Body>, Error> {
    let transfer_enc_chunk = res
        .headers()
        .get("transfer-encoding")
        .map(|h| h == "chunked")
        .unwrap_or(false);

    let content_size = res
        .headers()
        .get("content-size")
        .and_then(|h| h.to_str().ok().and_then(|c| c.parse::<usize>().ok()));

    let use_chunked = transfer_enc_chunk || content_size.is_none();

    let inner = if use_chunked {
        let decode = ChunkedDecoder::new(stream);
        BodyImpl::Http11Chunked(decode)
    } else if let Some(content_size) = content_size {
        let limit = LimitRead::new(stream, content_size);
        BodyImpl::Http11Limited(limit)
    } else {
        BodyImpl::Http11Unlimited(stream)
    };

    let body = Body::new(inner);

    let (parts, _) = res.into_parts();

    Ok(http::Response::from_parts(parts, body))
}
