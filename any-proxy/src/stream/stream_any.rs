use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

#[cfg(feature = "anyproxy-openssl")]
use tokio_openssl::SslStream;
#[cfg(feature = "anyproxy-rustls")]
use tokio_rustls::server::TlsStream;

pub enum StreamAny {
    Quic((quinn::SendStream, quinn::RecvStream)),
    Tcp(TcpStream),
    #[cfg(feature = "anyproxy-openssl")]
    TcpTls(SslStream<TcpStream>),
    #[cfg(feature = "anyproxy-rustls")]
    TcpTls(TlsStream<TcpStream>),
    AnyT(any_tunnel::stream::Stream),
    AnyT2(any_tunnel2::stream::Stream),
}

impl tokio::io::AsyncRead for StreamAny {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut *self {
            StreamAny::Quic(stream) => {
                let (_, recv) = stream;
                return Pin::new(recv).poll_read(cx, buf);
            }
            StreamAny::Tcp(stream) => {
                return Pin::new(stream).poll_read(cx, buf);
            }
            StreamAny::TcpTls(stream) => {
                return Pin::new(stream).poll_read(cx, buf);
            }
            StreamAny::AnyT(stream) => {
                return Pin::new(stream).poll_read(cx, buf);
            }
            StreamAny::AnyT2(stream) => {
                return Pin::new(stream).poll_read(cx, buf);
            }
        }
    }
}

impl tokio::io::AsyncWrite for StreamAny {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            StreamAny::Quic(stream) => {
                let (send, _) = stream;
                return Pin::new(send).poll_write(cx, buf);
            }
            StreamAny::Tcp(stream) => return Pin::new(stream).poll_write(cx, buf),
            StreamAny::TcpTls(stream) => return Pin::new(stream).poll_write(cx, buf),
            StreamAny::AnyT(stream) => return Pin::new(stream).poll_write(cx, buf),
            StreamAny::AnyT2(stream) => return Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            StreamAny::Quic(stream) => {
                let (send, _) = stream;
                Pin::new(send).poll_flush(cx)
            }
            StreamAny::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            StreamAny::TcpTls(stream) => Pin::new(stream).poll_flush(cx),
            StreamAny::AnyT(stream) => Pin::new(stream).poll_flush(cx),
            StreamAny::AnyT2(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            StreamAny::Quic(stream) => {
                let (send, _) = stream;
                Pin::new(send).poll_shutdown(cx)
            }
            StreamAny::Tcp(stream) => Pin::new(stream).poll_shutdown(cx),
            StreamAny::TcpTls(stream) => Pin::new(stream).poll_shutdown(cx),
            StreamAny::AnyT(stream) => Pin::new(stream).poll_shutdown(cx),
            StreamAny::AnyT2(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}
