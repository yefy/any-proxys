use any_base::io::async_write_msg::AsyncWriteBuf;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;
#[cfg(feature = "anyproxy-openssl")]
use tokio_openssl::SslStream;
use tokio_rustls::client::TlsStream as ClientTlsStream;
use tokio_rustls::server::TlsStream as ServerTlsStream;

pub enum StreamData {
    C(ClientTlsStream<TcpStream>),
    S(ServerTlsStream<TcpStream>),
    #[cfg(feature = "anyproxy-openssl")]
    Openssl(SslStream<TcpStream>),
}

pub struct Stream {
    stream_data: StreamData,
}

impl Stream {
    pub fn new(stream_data: StreamData) -> Stream {
        Stream { stream_data }
    }
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
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
        match &mut self.stream_data {
            StreamData::C(data) => Pin::new(data).poll_read(cx, buf),
            StreamData::S(data) => Pin::new(data).poll_read(cx, buf),
            #[cfg(feature = "anyproxy-openssl")]
            StreamData::Openssl(data) => Pin::new(data).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.stream_data {
            StreamData::C(data) => Pin::new(data).poll_write(cx, buf),
            StreamData::S(data) => Pin::new(data).poll_write(cx, buf),
            #[cfg(feature = "anyproxy-openssl")]
            StreamData::Openssl(data) => Pin::new(data).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.stream_data {
            StreamData::C(data) => Pin::new(data).poll_flush(cx),
            StreamData::S(data) => Pin::new(data).poll_flush(cx),
            #[cfg(feature = "anyproxy-openssl")]
            StreamData::Openssl(data) => Pin::new(data).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.stream_data {
            StreamData::C(data) => Pin::new(data).poll_shutdown(cx),
            StreamData::S(data) => Pin::new(data).poll_shutdown(cx),
            #[cfg(feature = "anyproxy-openssl")]
            StreamData::Openssl(data) => Pin::new(data).poll_shutdown(cx),
        }
    }
}
