use crate::io::async_write_msg::AsyncWriteBuf;
use crate::util::StreamReadMsg;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait StreamReadTokio: AsyncRead + Unpin {}
impl<T: AsyncRead + Unpin> StreamReadTokio for T {}

pub trait StreamWriteTokio: AsyncWrite + Unpin {}
impl<T: AsyncWrite + Unpin> StreamWriteTokio for T {}

pub trait StreamReadWriteTokio: StreamReadTokio + StreamWriteTokio + Unpin {}
impl<T: StreamReadTokio + StreamWriteTokio + Unpin> StreamReadWriteTokio for T {}

pub struct Stream {
    r: Box<dyn StreamReadTokio>,
    w: Box<dyn StreamWriteTokio>,
}

unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

impl Stream {
    pub fn new<RW: StreamReadWriteTokio + 'static>(rw: RW) -> Stream {
        let (r, w) = tokio::io::split(rw);
        Stream {
            r: Box::new(r),
            w: Box::new(w),
        }
    }
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
        return Poll::Ready(Ok(StreamReadMsg::new()));
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        return Poll::Ready(Ok(StreamReadMsg::new()));
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_try_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        return Poll::Ready(io::Result::Ok(()));
    }

    fn is_read_msg(&self) -> bool {
        false
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
        return Poll::Ready(false);
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
        false
    }
    fn write_cache_size(&self) -> usize {
        0
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.r).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.w).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.w).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.w).poll_shutdown(cx)
    }
}
