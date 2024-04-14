use super::stream_channel_read;
use crate::io::async_write_msg::{AsyncWriteBuf, MsgWriteBuf};
use crate::util::StreamReadMsg;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Stream {
    stream_tx: Option<async_channel::Sender<MsgWriteBuf>>,
    stream_tx_data_size: usize,
    stream_tx_future: Option<
        Pin<
            Box<
                dyn Future<Output = std::result::Result<(), async_channel::SendError<MsgWriteBuf>>>
                    + std::marker::Send
                    + Sync,
            >,
        >,
    >,
}

impl Stream {
    pub fn new(stream_tx: async_channel::Sender<MsgWriteBuf>) -> Stream {
        Stream {
            stream_tx_data_size: 0,
            stream_tx: Some(stream_tx),
            stream_tx_future: None,
        }
    }

    pub fn bounded(cap: usize) -> (Stream, stream_channel_read::Stream) {
        let (stream_tx, stream_rx) = async_channel::bounded(cap);
        (
            Stream {
                stream_tx_data_size: 0,
                stream_tx: Some(stream_tx),
                stream_tx_future: None,
            },
            stream_channel_read::Stream::new(stream_rx),
        )
    }

    pub fn close(&mut self) {
        self.write_close();
    }

    pub fn is_write_close(&self) -> bool {
        self.stream_tx.is_none()
    }

    pub fn write_close(&mut self) {
        let stream_tx = self.stream_tx.take();
        if stream_tx.is_some() {
            log::debug!(target: "main", "stream write_close");
            let stream_tx = stream_tx.unwrap();
            stream_tx.close();
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.close()
    }
}

impl crate::io::async_stream::Stream for Stream {
    fn raw_fd(&self) -> i32 {
        0
    }
    fn is_sendfile(&self) -> bool {
        false
    }
}

impl crate::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_raw_fd(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        return Poll::Ready(0);
    }
}

impl crate::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let kind = io::ErrorKind::ConnectionReset;
        let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
        return Poll::Ready(ret);
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let kind = io::ErrorKind::ConnectionReset;
        let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
        return Poll::Ready(ret);
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(true);
    }

    fn poll_try_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let kind = io::ErrorKind::ConnectionReset;
        let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
        return Poll::Ready(ret);
    }

    fn is_read_msg(&self) -> bool {
        true
    }
    fn read_cache_size(&self) -> usize {
        0
    }
}

impl crate::io::async_write_msg::AsyncWriteMsg for Stream {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        if self.is_write_close() {
            return Poll::Ready(Ok(0));
        }

        let mut stream_tx_future = if self.stream_tx_future.is_some() {
            self.stream_tx_future.take().unwrap()
        } else {
            self.stream_tx_data_size = buf.len();

            let data = buf.data();
            let sender = { self.stream_tx.clone().unwrap() };

            Box::pin(async move { sender.send(data).await })
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
    fn poll_write_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        return Poll::Ready(io::Result::Ok(()));
    }
    fn poll_sendfile(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _file_fd: i32,
        _seek: u64,
        _size: usize,
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(0))
    }

    fn is_write_msg(&self) -> bool {
        true
    }
    fn write_cache_size(&self) -> usize {
        0
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        return Poll::Ready(Ok(()));
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        return Poll::Ready(Ok(0));
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.write_close();
        Poll::Ready(Ok(()))
    }
}
