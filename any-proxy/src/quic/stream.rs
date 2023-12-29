use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::util::StreamMsg;
use anyhow::anyhow;
use anyhow::Result;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Stream {
    stream_rx: quinn::RecvStream,
    // stream_rx_future: Option<
    //     Pin<
    //         Box<dyn Future<Output = Result<(Bytes, quinn::RecvStream)>> + std::marker::Send + Sync>,
    //     >,
    // >,
    stream_tx: Option<quinn::SendStream>,
    stream_tx_data_size: usize,
    stream_tx_future:
        Option<Pin<Box<dyn Future<Output = Result<quinn::SendStream>> + std::marker::Send + Sync>>>,
}

impl Stream {
    pub fn new(stream_rx: quinn::RecvStream, stream_tx: quinn::SendStream) -> Stream {
        Stream {
            stream_rx,
            //stream_rx_future: None,
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
    fn poll_write_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        return Poll::Ready(io::Result::Ok(()));
    }
    fn poll_try_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        use tokio::io::AsyncRead;
        let ret = Pin::new(&mut self.stream_rx).poll_read(cx, buf);
        match ret {
            Poll::Pending => {
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(ret) => {
                if ret.is_err() {
                    ret?;
                }
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamMsg>> {
        let mut is_err = false;
        let mut stream_msg = StreamMsg::new();
        loop {
            if is_err {
                if stream_msg.len() <= 0 {
                    let kind = io::ErrorKind::ConnectionReset;
                    let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
                    return Poll::Ready(ret);
                }
                return Poll::Ready(Ok(stream_msg));
            }

            let mut read_chunk = Box::pin(self.stream_rx.read_chunk(msg_size, true));
            let ret = Pin::new(&mut read_chunk).poll(cx);
            match ret {
                Poll::Ready(Err(_)) => {
                    is_err = true;
                    continue;
                }
                Poll::Ready(Ok(None)) => {
                    is_err = true;
                    continue;
                }
                Poll::Ready(Ok(Some(data))) => {
                    stream_msg.push(data.bytes.into());
                    if stream_msg.len() >= msg_size {
                        return Poll::Ready(Ok(stream_msg));
                    }
                }
                Poll::Pending => {
                    return Poll::Ready(Ok(stream_msg));
                }
            }
        }
    }

    fn poll_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamMsg>> {
        /*
        let mut is_err = false;
        let mut value: Option<Vec<u8>> = None;
        loop {
            if is_err {
                if value.is_some() {
                    return Poll::Ready(Ok(value.unwrap()));
                } else {
                    return Poll::Ready(Ok(Vec::new()));
                }
            }
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
                    let data = stream_rx.read_chunk(msg_size, true).await?;
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
                    is_err = true;
                    continue;
                }
                Poll::Ready(Ok((data, stream_rx))) => {
                    self.stream_rx = Some(stream_rx);
                    if value.is_none() {
                        value = Some(data.into())
                    } else {
                        value
                            .as_mut()
                            .unwrap()
                            .extend_from_slice(data.into().as_slice());
                    }

                    if value.as_ref().unwrap().len() >= msg_size {
                        return Poll::Ready(Ok(value.unwrap()));
                    }
                }
                Poll::Pending => {
                    //Pending的时候保存起来
                    self.stream_rx_future = Some(stream_rx_future);
                    if value.is_some() {
                        return Poll::Ready(Ok(value.unwrap()));
                    } else {
                        return Poll::Pending;
                    }
                }
            }
        }

         */

        let mut is_err = false;
        let mut stream_msg = StreamMsg::new();
        loop {
            if is_err {
                return Poll::Ready(Ok(stream_msg));
            }

            let mut read_chunk = Box::pin(self.stream_rx.read_chunk(msg_size, true));
            let ret = Pin::new(&mut read_chunk).poll(cx);
            match ret {
                Poll::Ready(Err(_)) => {
                    is_err = true;
                    continue;
                }
                Poll::Ready(Ok(None)) => {
                    is_err = true;
                    continue;
                }
                Poll::Ready(Ok(Some(data))) => {
                    stream_msg.push(data.bytes.into());
                    if stream_msg.len() >= msg_size {
                        return Poll::Ready(Ok(stream_msg));
                    }
                }
                Poll::Pending => {
                    if stream_msg.len() > 0 {
                        return Poll::Ready(Ok(stream_msg));
                    } else {
                        return Poll::Pending;
                    }
                }
            }
        }
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(true);
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
        return Poll::Ready(true);
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream_rx).poll_read(cx, buf)
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
        if self.stream_tx.is_some() {
            Pin::new(&mut self.stream_tx.as_mut().unwrap()).poll_flush(cx)
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.stream_tx.is_some() {
            Pin::new(&mut self.stream_tx.as_mut().unwrap()).poll_shutdown(cx)
        } else {
            Poll::Ready(Ok(()))
        }
    }
}
