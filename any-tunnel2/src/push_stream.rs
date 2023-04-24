use any_base::io::async_write_msg::AsyncWriteBuf;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct PushStream {
    r: Box<dyn AsyncRead + Send + Unpin>,
    w: Box<dyn AsyncWrite + Send + Unpin>,
}

impl PushStream {
    pub fn new(
        r: Box<dyn AsyncRead + Send + Unpin>,
        w: Box<dyn AsyncWrite + Send + Unpin>,
    ) -> PushStream {
        PushStream { r, w }
    }
}

impl any_base::io::async_stream::AsyncStream for PushStream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for PushStream {
    fn poll_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
        return Poll::Ready(Ok(Vec::new()));
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for PushStream {
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

impl tokio::io::AsyncRead for PushStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.r).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for PushStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.w).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.w).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.w).poll_shutdown(cx)
    }
}
