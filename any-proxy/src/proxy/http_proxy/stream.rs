use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::typ::ArcMutexTokio;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use hyper::body::HttpBody;
use hyper::body::Sender;
use hyper::Body;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

struct StreamBuf {
    data: bytes::Bytes,
    pos: usize,
}

impl StreamBuf {
    fn new(data: bytes::Bytes) -> StreamBuf {
        StreamBuf { data, pos: 0 }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    #[inline]
    fn split_to(&mut self, at: usize) -> &[u8] {
        let mut end = self.pos + at;
        if end > self.data.len() {
            end = self.data.len();
        }
        let pos = self.pos;
        self.pos = end;
        &self.data.as_ref()[pos..end]
    }

    #[inline]
    fn take(self) -> Option<Vec<u8>> {
        let StreamBuf { data, pos } = self;
        if pos <= 0 {
            return Some(data.to_vec());
        }

        if pos >= data.len() {
            return None;
        }

        Some(data.as_ref()[pos..].to_vec())
    }
}

pub struct Stream {
    stream_rx: Option<ArcMutexTokio<Body>>,
    stream_rx_buf: Option<StreamBuf>,
    stream_rx_future:
        Option<Pin<Box<dyn Future<Output = Result<Bytes>> + std::marker::Send + Sync>>>,
    stream_tx: Option<ArcMutexTokio<Sender>>,
    stream_tx_data_size: usize,
    stream_tx_future: Option<Pin<Box<dyn Future<Output = Result<()>> + std::marker::Send + Sync>>>,
}

impl Stream {
    pub fn new(stream_rx: Body, stream_tx: Sender) -> Stream {
        let stream_rx = ArcMutexTokio::new(stream_rx);
        let stream_tx = ArcMutexTokio::new(stream_tx);
        Stream {
            stream_rx: Some(stream_rx),
            stream_rx_buf: None,
            stream_rx_future: None,
            stream_tx_future: None,
            stream_tx_data_size: 0,
            stream_tx: Some(stream_tx),
        }
    }

    pub fn close(&mut self) {
        self.write_close();
        self.read_close();
    }

    pub fn is_read_close(&self) -> bool {
        self.stream_rx.is_none()
    }

    pub fn is_write_close(&self) -> bool {
        self.stream_tx.is_none()
    }

    pub fn read_close(&mut self) {
        let stream_rx = self.stream_rx.take();
        if stream_rx.is_some() {
            log::debug!("stream read_close");
        }
    }

    pub fn write_close(&mut self) {
        let stream_tx = self.stream_tx.take();
        if stream_tx.is_some() {
            log::debug!("stream write_close");
        }
    }

    fn read_buf(&mut self, buf: &mut tokio::io::ReadBuf<'_>) -> bool {
        if self.stream_rx_buf.is_some() {
            let cache_buf = self.stream_rx_buf.as_mut().unwrap();
            let remain = cache_buf.remaining();
            if remain > 0 {
                let expected = buf.initialize_unfilled().len();
                let split_at = std::cmp::min(expected, remain);
                let data = cache_buf.split_to(split_at);
                buf.put_slice(data);
                true
            } else {
                self.stream_rx_buf = None;
                false
            }
        } else {
            false
        }
    }

    fn read_buf_take(&mut self) -> Option<Vec<u8>> {
        if self.stream_rx_buf.is_some() {
            let cache_buf = self.stream_rx_buf.take().unwrap();
            cache_buf.take()
        } else {
            None
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.close()
    }
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(true);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_read_msg(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
        if self.is_read_close() {
            //log::error!("err:is_read_close");
            return Poll::Ready(Ok(Vec::new()));
        }

        let datas = self.read_buf_take();
        if datas.is_some() {
            return Poll::Ready(Ok(datas.unwrap()));
        }

        let mut stream_rx_future = if self.stream_rx_future.is_some() {
            self.stream_rx_future.take().unwrap()
        } else {
            let stream_rx = self.stream_rx.clone().unwrap();
            Box::pin(async move {
                let data = stream_rx.get_mut().await.data().await;
                if data.is_none() {
                    return Err(anyhow!("err:stream_rx close"));
                }
                let data = data.unwrap();
                match data {
                    Err(e) => {
                        return Err(anyhow!("err:stream_rx close => e:{}", e));
                    }
                    Ok(data) => {
                        return Ok(data);
                    }
                }
            })
        };

        let ret = stream_rx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                self.read_close();
                return Poll::Ready(Ok(Vec::new()));
            }
            Poll::Ready(Ok(data)) => {
                self.stream_rx_buf = Some(StreamBuf::new(data));

                let datas = self.read_buf_take();
                return Poll::Ready(Ok(datas.unwrap()));
            }
            Poll::Pending => {
                //Pending的时候保存起来
                self.stream_rx_future = Some(stream_rx_future);
                return Poll::Pending;
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
        if self.is_write_close() {
            //log::error!("err:is_write_close");
            return Poll::Ready(Ok(0));
        }

        let mut stream_tx_future = if self.stream_tx_future.is_some() {
            self.stream_tx_future.take().unwrap()
        } else {
            self.stream_tx_data_size = buf.len();

            let data = bytes::Bytes::from(buf.data());
            let stream_tx = self.stream_tx.clone().unwrap();
            Box::pin(async move {
                stream_tx
                    .get_mut()
                    .await
                    .send_data(data)
                    .await
                    .map_err(|e| anyhow!("err:send_data => e:{}", e))
            })
        };

        let ret = stream_tx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                self.write_close();
                return Poll::Ready(Ok(0));
            }
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(self.stream_tx_data_size)),
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
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.is_read_close() {
            //log::error!("err:is_read_close");
            return Poll::Ready(Ok(()));
        }

        if self.read_buf(buf) {
            return Poll::Ready(Ok(()));
        }

        let mut stream_rx_future = if self.stream_rx_future.is_some() {
            self.stream_rx_future.take().unwrap()
        } else {
            let stream_rx = self.stream_rx.clone().unwrap();
            Box::pin(async move {
                let data = stream_rx.get_mut().await.data().await;
                if data.is_none() {
                    return Err(anyhow!("err:stream_rx close"));
                }
                let data = data.unwrap();
                match data {
                    Err(e) => {
                        return Err(anyhow!("err:stream_rx close => e:{}", e));
                    }
                    Ok(data) => {
                        //log::info!("recv size {}", data.len());
                        return Ok(data);
                    }
                }
            })
        };

        let ret = stream_rx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                self.read_close();
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(Ok(data)) => {
                self.stream_rx_buf = Some(StreamBuf::new(data));

                let _ = self.read_buf(buf);
                return Poll::Ready(Ok(()));
            }
            Poll::Pending => {
                //Pending的时候保存起来
                self.stream_rx_future = Some(stream_rx_future);
                return Poll::Pending;
            }
        }
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.is_write_close() {
            //log::error!("err:is_write_close");
            return Poll::Ready(Ok(0));
        }

        let mut stream_tx_future = if self.stream_tx_future.is_some() {
            self.stream_tx_future.take().unwrap()
        } else {
            self.stream_tx_data_size = buf.len();

            let data = bytes::Bytes::from(buf.to_vec());
            //log::info!("send size {}", data.len());

            let stream_tx = self.stream_tx.clone().unwrap();
            Box::pin(async move {
                stream_tx
                    .get_mut()
                    .await
                    .send_data(data)
                    .await
                    .map_err(|e| anyhow!("err:send_data => e:{}", e))
            })
        };

        let ret = stream_tx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                self.write_close();
                return Poll::Ready(Ok(0));
            }
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(self.stream_tx_data_size)),
            Poll::Pending => {
                //Pending的时候保存起来
                self.stream_tx_future = Some(stream_tx_future);
                return Poll::Pending;
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.is_write_close() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "ConnectionReset",
            )));
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.close();
        Poll::Ready(Ok(()))
    }
}
