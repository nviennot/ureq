use crate::Stream;
use crate::{AsyncRead, AsyncReadExt, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Peekable<S> {
    wrapped: S,
    buffer: Vec<u8>,
}

impl<S: Stream> Peekable<S> {
    pub fn new(wrapped: S, capacity: usize) -> Self {
        Peekable {
            wrapped,
            buffer: Vec::with_capacity(capacity),
        }
    }

    pub async fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // check if we have enough in the buffer for the peek already
        if self.buffer.len() < buf.len() {
            let already_read = self.buffer.len(); // we got this much already
            self.buffer.resize(buf.len(), 0); // ensure we have space
            let read_into = &mut self.buffer[already_read..buf.len()];
            let read_amount = self.wrapped.read(read_into).await?;
            if read_amount < read_into.len() {
                // we got less than needed, resize down
                self.buffer.resize(already_read + read_amount, 0);
            }
        }
        let max = buf.len().min(self.buffer.len());
        (&mut buf[0..max]).copy_from_slice(&self.buffer[0..max]);
        Ok(max)
    }
}

impl<S: Stream> AsyncRead for Peekable<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // if we have buffered, provide that first.
        if !self.buffer.is_empty() {
            let max = self.buffer.len().min(buf.len());
            {
                let s = self.get_mut();
                (&mut buf[0..max]).copy_from_slice(&s.buffer[0..max]);
                s.buffer.drain(0..max);
            };
            return Poll::Ready(Ok(max));
        }

        // nothing more buffered, delegate to wrapped
        Pin::new(&mut self.get_mut().wrapped).poll_read(cx, buf)
    }
}

/// Boilerplate below

impl<S: Stream> AsyncWrite for Peekable<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().wrapped).poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().wrapped).poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().wrapped).poll_close(cx)
    }
}

impl<S: Stream> Stream for Peekable<S> {}
