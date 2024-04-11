use super::stream_channel_write;
use crate::io::async_write_msg::{AsyncWriteBuf, MsgWriteBuf};
use crate::util::StreamReadMsg;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Stream {
    stream_rx: Option<async_channel::Receiver<MsgWriteBuf>>,
    stream_rx_future: Option<
        Pin<
            Box<
                dyn Future<Output = std::result::Result<MsgWriteBuf, async_channel::RecvError>>
                    + std::marker::Send
                    + Sync,
            >,
        >,
    >,
}

impl Stream {
    pub fn new(stream_rx: async_channel::Receiver<MsgWriteBuf>) -> Stream {
        Stream {
            stream_rx: Some(stream_rx),
            stream_rx_future: None,
        }
    }

    pub fn bounded(cap: usize) -> (stream_channel_write::Stream, Stream) {
        let (stream_tx, stream_rx) = async_channel::bounded(cap);
        (
            stream_channel_write::Stream::new(stream_tx),
            Stream {
                stream_rx: Some(stream_rx),
                stream_rx_future: None,
            },
        )
    }

    pub fn close(&mut self) {
        self.read_close();
    }

    pub fn is_read_close(&self) -> bool {
        self.stream_rx.is_none()
    }

    pub fn read_close(&mut self) {
        let stream_rx = self.stream_rx.take();
        if stream_rx.is_some() {
            log::debug!("stream read_close");
            let stream_rx = stream_rx.unwrap();
            stream_rx.close();
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
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let mut stream_msg = StreamReadMsg::new();

        loop {
            if stream_msg.is_file() || stream_msg.data_len() >= msg_size {
                return Poll::Ready(Ok(stream_msg));
            }

            if self.is_read_close() {
                if stream_msg.is_empty() {
                    let kind = io::ErrorKind::ConnectionReset;
                    let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
                    return Poll::Ready(ret);
                }
                return Poll::Ready(Ok(stream_msg));
            }

            match self.stream_rx.as_ref().unwrap().try_recv() {
                Ok(data) => {
                    stream_msg.push_back_msg(data);
                    continue;
                }
                Err(async_channel::TryRecvError::Empty) => {
                    return Poll::Ready(Ok(stream_msg));
                }
                Err(async_channel::TryRecvError::Closed) => {
                    self.read_close();
                    continue;
                }
            }
        }
    }
    fn poll_read_msg(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let mut stream_msg = StreamReadMsg::new();

        loop {
            if stream_msg.is_file() || stream_msg.data_len() >= msg_size {
                return Poll::Ready(Ok(stream_msg));
            }

            if self.is_read_close() {
                return Poll::Ready(Ok(stream_msg));
            }

            let mut stream_rx_future = if self.stream_rx_future.is_some() {
                self.stream_rx_future.take().unwrap()
            } else {
                match self.stream_rx.as_ref().unwrap().try_recv() {
                    Ok(data) => {
                        stream_msg.push_back_msg(data);
                        continue;
                    }
                    Err(async_channel::TryRecvError::Empty) => {}
                    Err(async_channel::TryRecvError::Closed) => {
                        self.read_close();
                        continue;
                    }
                }

                if stream_msg.data_len() > 0 {
                    return Poll::Ready(Ok(stream_msg));
                }

                let stream_rx = self.stream_rx.clone().unwrap();
                Box::pin(async move { stream_rx.recv().await })
            };

            let ret = stream_rx_future.as_mut().poll(_cx);
            match ret {
                Poll::Ready(Err(_)) => {
                    self.read_close();
                    continue;
                }
                Poll::Ready(Ok(data)) => {
                    stream_msg.push_back_msg(data);
                    continue;
                }
                Poll::Pending => {
                    //Pending的时候保存起来
                    self.stream_rx_future = Some(stream_rx_future);
                    if stream_msg.data_len() > 0 {
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

    fn poll_try_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        return Poll::Ready(Ok(()));
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
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        return Poll::Ready(Ok(0));
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

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
