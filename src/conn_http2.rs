use crate::body::{Body, BodyImpl};
use crate::Error;
use bytes::Bytes;
use h2::client::SendRequest;

const BUF_SIZE: usize = 16_384;

pub async fn send_request_http2(
    send_req: SendRequest<Bytes>,
    req: http::Request<Body>,
) -> Result<http::Response<Body>, Error> {
    //
    let send_req_clone = send_req.clone();

    println!("-- wait for h2.ready()");
    let mut h2 = send_req.ready().await?;

    let (parts, mut body_read) = req.into_parts();
    let req = http::Request::from_parts(parts, ());

    println!("-- h2.send_request()");

    let (fut_res, mut send_body) = h2.send_request(req, false)?;

    println!("-- h2 send body");

    let mut buf = vec![0_u8; BUF_SIZE];
    loop {
        let amount_read = body_read.read(&mut buf[..]).await?;
        if amount_read == 0 {
            break;
        }
        let mut amount_sent = 0;
        loop {
            let left_to_send = amount_read - amount_sent;
            send_body.reserve_capacity(left_to_send);
            let fut_cap = PollCapacity(&mut send_body);
            let actual_capacity = fut_cap.await?;
            send_body.send_data(
                // h2::SendStream lacks a sync or async function that allows us
                // to send borrowed data. This copy is unfortunate.
                // TODO contact h2 and ask if they would consider some kind of
                // async variant that takes a &mut [u8].
                Bytes::copy_from_slice(&buf[amount_sent..(amount_sent + actual_capacity)]),
                false,
            )?;
            amount_sent += actual_capacity;
        }
    }

    // Send end_of_stream
    send_body.send_data(Bytes::new(), true)?;

    println!("-- h2 body sent, wait for response future");

    let (parts, res_body) = fut_res.await?.into_parts();

    println!("-- h2 got response");

    let res_body = Body::new(BodyImpl::Http2(res_body, send_req_clone));
    let res = http::Response::from_parts(parts, res_body);

    Ok(res)
}

struct PollCapacity<'a>(&'a mut h2::SendStream<Bytes>);

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

impl<'a> Future for PollCapacity<'a> {
    type Output = Result<usize, h2::Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.get_mut().0).poll_capacity(cx) {
            Poll::Ready(Some(res)) => Poll::Ready(res),
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }
}
