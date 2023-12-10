use any_base::io::async_write_msg::AsyncWriteBuf;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

pub struct Stream {
    s: TcpStream,
}

impl Stream {
    pub fn new(s: TcpStream) -> Stream {
        Stream { s }
    }
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&self.s).poll_write_ready(cx)
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<Vec<u8>>> {
        return Poll::Ready(Ok(Vec::new()));
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for Stream {
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
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.s).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.s).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.s.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_shutdown(cx)
    }
}
