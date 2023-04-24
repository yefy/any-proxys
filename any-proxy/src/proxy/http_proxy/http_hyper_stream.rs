use any_base::stream_flow::StreamFlow;
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
