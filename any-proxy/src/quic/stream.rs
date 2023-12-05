use any_base::io::async_write_msg::AsyncWriteBuf;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Stream {
    stream_rx: Option<quinn::RecvStream>,
    stream_rx_future: Option<
        Pin<
            Box<dyn Future<Output = Result<(Bytes, quinn::RecvStream)>> + std::marker::Send + Sync>,
        >,
    >,
    stream_tx: Option<quinn::SendStream>,
    stream_tx_data_size: usize,
    stream_tx_future:
        Option<Pin<Box<dyn Future<Output = Result<quinn::SendStream>> + std::marker::Send + Sync>>>,
}

impl Stream {
    pub fn new(stream_rx: quinn::RecvStream, stream_tx: quinn::SendStream) -> Stream {
        Stream {
            stream_rx: Some(stream_rx),
            stream_rx_future: None,
            stream_tx_future: None,
            stream_tx_data_size: 0,
            stream_tx: Some(stream_tx),
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {}
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_read_msg(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
        let mut stream_rx_future = if self.stream_rx_future.is_some() {
            self.stream_rx_future.take().unwrap()
        } else {
            let stream_rx = self.stream_rx.take();
            if stream_rx.is_none() {
                log::error!("err:quic stream_rx.is_none");
                return Poll::Ready(Ok(Vec::new()));
            }
            let mut stream_rx = stream_rx.unwrap();

            Box::pin(async move {
                let data = stream_rx.read_chunk(usize::MAX, true).await?;
                if data.is_none() {
                    return Err(anyhow!("err:stream_rx close"));
                }
                let data = data.unwrap();
                Ok((data.bytes, stream_rx))
            })
        };

        let ret = stream_rx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                return Poll::Ready(Ok(Vec::new()));
            }
            Poll::Ready(Ok((data, stream_rx))) => {
                self.stream_rx = Some(stream_rx);
                return Poll::Ready(Ok(data.to_vec()));
            }
            Poll::Pending => {
                //Pending的时候保存起来
                self.stream_rx_future = Some(stream_rx_future);
                return Poll::Pending;
            }
        }
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for Stream {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        let mut stream_tx_future = if self.stream_tx_future.is_some() {
            self.stream_tx_future.take().unwrap()
        } else {
            self.stream_tx_data_size = buf.len();

            let data = bytes::Bytes::from(buf.data());
            let stream_tx = self.stream_tx.take();
            if stream_tx.is_none() {
                log::error!("err:quic stream_tx.is_none");
                return Poll::Ready(Ok(0));
            }
            let mut stream_tx = stream_tx.unwrap();
            Box::pin(async move {
                stream_tx
                    .write_chunk(data)
                    .await
                    .map_err(|e| anyhow!("err:send_data => e:{}", e))?;
                Ok(stream_tx)
            })
        };

        let ret = stream_tx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                return Poll::Ready(Ok(0));
            }
            Poll::Ready(Ok(stream_tx)) => {
                self.stream_tx = Some(stream_tx);
                Poll::Ready(Ok(self.stream_tx_data_size))
            }
            Poll::Pending => {
                //Pending的时候保存起来
                self.stream_tx_future = Some(stream_tx_future);
                return Poll::Pending;
            }
        }
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
        Pin::new(&mut self.stream_rx.as_mut().unwrap()).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream_tx.as_mut().unwrap()).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream_tx.as_mut().unwrap()).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream_tx.as_mut().unwrap()).poll_shutdown(cx)
    }
}
