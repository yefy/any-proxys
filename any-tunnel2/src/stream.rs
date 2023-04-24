use super::protopack::TunnelData;
use crate::anychannel::AnyAsyncReceiver;
use crate::StreamPackToStreamReceiver;
use any_base::io::async_write_msg::AsyncWriteBuf;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

struct StreamBuf {
    tunnel_data: Arc<TunnelData>,
    pos: usize,
}

impl StreamBuf {
    fn new(tunnel_data: Arc<TunnelData>) -> StreamBuf {
        StreamBuf {
            tunnel_data,
            pos: 0,
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.tunnel_data.datas.len() - self.pos
    }

    #[inline]
    fn split_to(&mut self, at: usize) -> &[u8] {
        let mut end = self.pos + at;
        if end > self.tunnel_data.datas.len() {
            end = self.tunnel_data.datas.len();
        }
        let pos = self.pos;
        self.pos = end;
        &self.tunnel_data.datas.as_slice()[pos..end]
    }
}

pub struct Stream {
    stream_tx: Option<async_channel::Sender<Vec<u8>>>,
    stream_rx: Option<StreamPackToStreamReceiver>,
    buf: Option<StreamBuf>,
    send: Option<
        Pin<
            Box<
                dyn Future<Output = std::result::Result<(), async_channel::SendError<Vec<u8>>>>
                    + std::marker::Send,
            >,
        >,
    >,
    send_len: usize,
}

impl Stream {
    pub fn new(
        stream_tx: async_channel::Sender<Vec<u8>>,
        stream_rx: StreamPackToStreamReceiver,
    ) -> Stream {
        Stream {
            stream_tx: Some(stream_tx),
            stream_rx: Some(stream_rx),
            buf: None,
            send: None,
            send_len: 0,
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
            log::debug!("close stream_rx");
            let stream_rx = stream_rx.unwrap();
            stream_rx.close();
            std::mem::drop(stream_rx);
        }
    }

    pub fn write_close(&mut self) {
        let stream_tx = self.stream_tx.take();
        if stream_tx.is_some() {
            log::debug!("close stream_tx");
            let stream_tx = stream_tx.unwrap();
            stream_tx.close();
            std::mem::drop(stream_tx);
        }
    }

    async fn read_channel(&mut self, buf: &mut tokio::io::ReadBuf<'_>) -> io::Result<()> {
        if self.stream_rx.is_none() {
            return Ok(());
        }
        loop {
            if self.buf.is_some() {
                let cache_buf = self.buf.as_mut().unwrap();
                let remain = cache_buf.remaining();
                if remain > 0 {
                    let expected = buf.initialize_unfilled().len();
                    let split_at = std::cmp::min(expected, remain);
                    let data = cache_buf.split_to(split_at);
                    buf.put_slice(data);
                    return Ok(());
                } else {
                    self.buf = None;
                }
            }

            let slice = self.stream_rx.as_mut().unwrap().recv().await;
            if slice.is_err() {
                self.close();
                return Ok(());
            }
            let slice = slice.unwrap();
            self.buf = Some(StreamBuf::new(slice));
        }
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
        let mut read_fut = Box::pin(self.read_channel(buf));
        let ret = read_fut.as_mut().poll(cx);
        match ret {
            Poll::Ready(ret) => Poll::Ready(ret),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.stream_tx.is_none() {
            return Poll::Ready(Ok(0));
        }

        let mut send = if self.send.is_some() {
            self.send.take().unwrap()
        } else {
            let len = buf.len();
            self.send_len = len;
            let mut slice = Vec::with_capacity(len);
            slice.extend_from_slice(buf);
            let stream_tx = self.stream_tx.clone().unwrap();
            Box::pin(async move { stream_tx.send(slice).await })
        };

        let send_len = self.send_len;
        let ret = send.as_mut().poll(cx);
        match ret {
            Poll::Ready(ret) => {
                if ret.is_err() {
                    self.close();
                    return Poll::Ready(Ok(0));
                }
                Poll::Ready(Ok(send_len))
            }
            Poll::Pending => {
                //Pending的时候保存起来
                self.send = Some(send);
                self.send_len = send_len;
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
        self.write_close();
        Poll::Ready(Ok(()))
    }
}
