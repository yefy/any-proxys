use super::stream_nil_write;
use crate::io::async_write_msg::AsyncWriteBuf;
use crate::util::StreamReadMsg;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Stream {}

impl Stream {
    pub fn new() -> Stream {
        Stream {}
    }

    pub fn bounded() -> (stream_nil_write::Stream, Stream) {
        (stream_nil_write::Stream::new(), Stream {})
    }
}

impl Drop for Stream {
    fn drop(&mut self) {}
}

impl crate::io::async_stream::Stream for Stream {
    fn raw_fd(&self) -> i32 {
        0
    }
    fn is_sendfile(&self) -> bool {
        false
    }
}

impl crate::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_raw_fd(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        return Poll::Ready(0);
    }
}

impl crate::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let kind = io::ErrorKind::ConnectionReset;
        let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
        return Poll::Ready(ret);
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let kind = io::ErrorKind::ConnectionReset;
        let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
        return Poll::Ready(ret);
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(true);
    }

    fn poll_try_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        return Poll::Ready(Ok(()));
    }

    fn is_read_msg(&self) -> bool {
        true
    }
    fn read_cache_size(&self) -> usize {
        0
    }
}

impl crate::io::async_write_msg::AsyncWriteMsg for Stream {
    fn poll_write_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        return Poll::Ready(Ok(0));
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(true);
    }
    fn poll_write_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        return Poll::Ready(io::Result::Ok(()));
    }
    fn poll_sendfile(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _file_fd: i32,
        _seek: u64,
        _size: usize,
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(0))
    }

    fn is_write_msg(&self) -> bool {
        true
    }
    fn write_cache_size(&self) -> usize {
        0
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        return Poll::Ready(Ok(()));
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        return Poll::Ready(Ok(0));
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
