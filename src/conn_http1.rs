use crate::body::{charset_from_headers, Body, BodyImpl, ContentEncoding};
use crate::h1::SendRequest;
use crate::Error;

const BUF_SIZE: usize = 16_384;

pub async fn send_request_http1(
    send_req: SendRequest,
    req: http::Request<Body>,
) -> Result<http::Response<Body>, Error> {
    //
    let send_req_clone = send_req.clone();

    let mut h1 = send_req; // .ready().await?;

    let (parts, mut body_read) = req.into_parts();
    let req = http::Request::from_parts(parts, ());

    let (fut_res, mut send_body) = h1.send_request(req, false)?;

    let mut buf = vec![0_u8; BUF_SIZE];
    loop {
        // wait for send_body to be able to receive more data
        send_body = send_body.ready().await?;
        let amount_read = body_read.read(&mut buf[..]).await?;
        if amount_read == 0 {
            break;
        }
        send_body.send_data(&buf[..amount_read], false)?;
    }

    // Send end_of_stream
    send_body.send_data(&[], true)?;

    let (parts, res_body) = fut_res.await?.into_parts();

    let content_encoding = ContentEncoding::from_headers(&parts.headers, true);
    let charset = charset_from_headers(&parts.headers);

    let mut res_body = Body::new(BodyImpl::Http1(res_body, send_req_clone), content_encoding);
    if let Some(charset) = charset {
        res_body.set_char_codec(charset, true);
    }

    let res = http::Response::from_parts(parts, res_body);

    Ok(res)
}
