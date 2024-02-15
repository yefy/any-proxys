use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::stream_flow::StreamFlow;
use any_base::util::StreamReadMsg;
use hyper::client::connect::{Connected, Connection};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct HttpHyperStream {
    stream: StreamFlow,
}

impl Connection for HttpHyperStream {
    fn connected(&self) -> Connected {
        let connected = Connected::new();
        connected
    }
}

impl HttpHyperStream {
    pub fn new(stream: StreamFlow) -> HttpHyperStream {
        HttpHyperStream { stream }
    }
}

impl any_base::io::async_stream::Stream for HttpHyperStream {
    fn raw_fd(&self) -> i32 {
        self.stream.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        self.raw_fd() > 0
    }
}
impl any_base::io::async_stream::AsyncStream for HttpHyperStream {
    fn poll_is_single(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.stream).poll_is_single(cx)
    }
    fn poll_raw_fd(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        Pin::new(&mut self.stream).poll_raw_fd(cx)
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for HttpHyperStream {
    fn poll_try_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        Pin::new(&mut self.stream).poll_try_read_msg(cx, msg_size)
    }
    fn poll_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        Pin::new(&mut self.stream).poll_read_msg(cx, msg_size)
    }

    fn poll_is_read_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.stream).poll_is_read_msg(cx)
    }
    fn poll_try_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_try_read(cx, buf)
    }

    fn is_read_msg(&self) -> bool {
        self.stream.is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        self.stream.read_cache_size()
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for HttpHyperStream {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_msg(cx, buf)
    }

    fn poll_is_write_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.stream).poll_is_write_msg(cx)
    }
    fn poll_write_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_write_ready(cx)
    }

    fn poll_sendfile(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_sendfile(cx, file_fd, seek, size)
    }

    fn is_write_msg(&self) -> bool {
        self.stream.is_write_msg()
    }
    fn write_cache_size(&self) -> usize {
        self.stream.write_cache_size()
    }
}

impl tokio::io::AsyncRead for HttpHyperStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for HttpHyperStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
