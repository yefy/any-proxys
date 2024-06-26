use super::protopack::DynamicTunnelData;
use super::protopack::TunnelPack;
use super::round_async_channel::RoundAsyncChannel;
#[cfg(feature = "anydebug")]
use crate::get_flag;
use crate::protopack::TunnelDataVec;
use crate::PeerClientToStreamReceiver;
use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::stream_buf::StreamBuf;
use any_base::typ::ArcMutex;
use any_base::util::ArcString;
use any_base::util::StreamReadMsg;
use awaitgroup::WorkerInner;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

pub struct Stream {
    is_client: bool,
    stream_rx: Option<PeerClientToStreamReceiver>,
    stream_rx_buf: StreamBuf,
    stream_rx_future: Option<
        Pin<
            Box<
                dyn Future<
                        Output = std::result::Result<DynamicTunnelData, async_channel::RecvError>,
                    > + std::marker::Send
                    + Sync,
            >,
        >,
    >,
    stream_tx: Option<ArcMutex<RoundAsyncChannel<TunnelPack>>>,
    stream_tx_data_size: usize,
    stream_tx_future: Option<
        Pin<
            Box<
                dyn Future<Output = std::result::Result<(), async_channel::SendError<TunnelPack>>>
                    + std::marker::Send
                    + Sync,
            >,
        >,
    >,
    _session_id: ArcString,
    stream_tx_pack_id: Arc<AtomicU32>,
    stream_rx_pack_id: Arc<AtomicU32>,
    worker_inner: Option<WorkerInner>,
}

impl Stream {
    pub fn new(
        is_client: bool,
        stream_rx: PeerClientToStreamReceiver,
        stream_tx: ArcMutex<RoundAsyncChannel<TunnelPack>>,
        _max_stream_size: usize,
        _channel_size: usize,
        _session_id: ArcString,
        stream_tx_pack_id: Arc<AtomicU32>,
        stream_rx_pack_id: Arc<AtomicU32>,
        worker_inner: WorkerInner,
    ) -> Stream {
        Stream {
            is_client,
            stream_rx: Some(stream_rx),
            stream_rx_buf: StreamBuf::default(),
            stream_rx_future: None,
            stream_tx_future: None,
            stream_tx_data_size: 0,
            stream_tx: Some(stream_tx),
            _session_id,
            stream_tx_pack_id,
            stream_rx_pack_id,
            worker_inner: Some(worker_inner),
        }
    }

    fn add_write_pack_id(&mut self) -> u32 {
        self.stream_tx_pack_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn close(&mut self) {
        if self.worker_inner.is_some() {
            let worker_inner = self.worker_inner.take().unwrap();
            worker_inner.done();
        }
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
            log::debug!(target: "main", "stream read_close");
            let stream_rx = stream_rx.unwrap();
            stream_rx.close();

            #[cfg(feature = "anydebug")]
            log::info!(
                "flag:{} session_id:{} stream read_close",
                get_flag(self.is_client),
                self._session_id
            );
        }
    }

    pub fn write_close(&mut self) {
        let stream_tx = self.stream_tx.take();
        if stream_tx.is_some() {
            log::debug!(target: "main", "stream write_close");
            let stream_tx = stream_tx.unwrap();
            {
                let stream_tx = &mut *stream_tx.get_mut();
                stream_tx.close();
            }

            #[cfg(feature = "anydebug")]
            log::info!(
                "flag:{} session_id:{} stream write_close",
                get_flag(self.is_client),
                self._session_id
            );
        }
    }

    fn read_buf(&mut self, buf: &mut tokio::io::ReadBuf<'_>) -> bool {
        if !self.stream_rx_buf.is_empty() {
            let remain = self.stream_rx_buf.remaining();
            let expected = buf.initialize_unfilled().len();
            let split_at = std::cmp::min(expected, remain);
            let data = self.stream_rx_buf.split_to(split_at);
            buf.put_slice(data.as_ref());
            true
        } else {
            false
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.close()
    }
}

impl any_base::io::async_stream::Stream for Stream {
    fn raw_fd(&self) -> i32 {
        0
    }
    fn is_sendfile(&self) -> bool {
        false
    }
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_raw_fd(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        return Poll::Ready(0);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        log::trace!(target: "main", "skip waning is_client:{}", self.is_client);
        let mut stream_msg = StreamReadMsg::new();

        loop {
            if self.is_read_close() {
                if stream_msg.is_empty() {
                    let kind = io::ErrorKind::ConnectionReset;
                    let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
                    return Poll::Ready(ret);
                }
                return Poll::Ready(Ok(stream_msg));
            }

            if !self.stream_rx_buf.is_empty() {
                stream_msg.push_back_data(self.stream_rx_buf.take());
                if stream_msg.data_len() >= msg_size {
                    return Poll::Ready(Ok(stream_msg));
                }
            }

            match self.stream_rx.as_ref().unwrap().try_recv() {
                Ok(tunnel_data) => {
                    self.stream_rx_pack_id
                        .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                    log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);

                    let DynamicTunnelData { header: _, datas } = tunnel_data;
                    self.stream_rx_buf.set_bytes(datas);
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
        log::trace!(target: "main", "skip waning is_client:{}", self.is_client);
        let mut stream_msg = StreamReadMsg::new();

        loop {
            if self.is_read_close() {
                return Poll::Ready(Ok(stream_msg));
            }

            if !self.stream_rx_buf.is_empty() {
                stream_msg.push_back_data(self.stream_rx_buf.take());
                if stream_msg.data_len() >= msg_size {
                    return Poll::Ready(Ok(stream_msg));
                }
            }

            let mut stream_rx_future = if self.stream_rx_future.is_some() {
                self.stream_rx_future.take().unwrap()
            } else {
                match self.stream_rx.as_ref().unwrap().try_recv() {
                    Ok(tunnel_data) => {
                        self.stream_rx_pack_id
                            .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                        log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);
                        let DynamicTunnelData { header: _, datas } = tunnel_data;
                        self.stream_rx_buf.set_bytes(datas);
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
                Poll::Ready(Ok(tunnel_data)) => {
                    self.stream_rx_pack_id
                        .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                    log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);
                    let DynamicTunnelData { header: _, datas } = tunnel_data;
                    self.stream_rx_buf.set_bytes(datas);
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
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        log::trace!(target: "main", "skip waning is_client:{}", self.is_client);

        let mut is_read = false;
        loop {
            if self.is_read_close() {
                if is_read {
                    return Poll::Ready(Ok(()));
                }
                let kind = io::ErrorKind::ConnectionReset;
                let ret = io::Result::Err(std::io::Error::new(kind, "err:read msg close"));
                return Poll::Ready(ret);
            }

            if self.read_buf(buf) {
                is_read = true;
                if buf.initialize_unfilled().len() <= 0 {
                    return Poll::Ready(Ok(()));
                }
            }

            match self.stream_rx.as_ref().unwrap().try_recv() {
                Ok(tunnel_data) => {
                    self.stream_rx_pack_id
                        .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                    log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);

                    let DynamicTunnelData { header: _, datas } = tunnel_data;
                    self.stream_rx_buf.set_bytes(datas);
                    continue;
                }
                Err(async_channel::TryRecvError::Empty) => {
                    return Poll::Ready(Ok(()));
                }
                Err(async_channel::TryRecvError::Closed) => {
                    self.read_close();
                    continue;
                }
            }
        }
    }

    fn is_read_msg(&self) -> bool {
        true
    }
    fn read_cache_size(&self) -> usize {
        self.stream_rx_buf.len()
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for Stream {
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

            let stream_tx_pack_id = self.add_write_pack_id();
            let data = buf.data().to_bytes();
            if data.is_none() {
                log::error!("err:data.is_none()");
                return Poll::Ready(Ok(0));
            }
            let data = data.unwrap();
            let mut tunnel_data = DynamicTunnelData::from(data);
            tunnel_data.header.pack_id = stream_tx_pack_id;
            tunnel_data.header.pack_size = tunnel_data.datas.len() as u32;
            log::trace!(target: "main", "write tunnel_data.header:{:?}", tunnel_data.header);

            let (sender, lock) = {
                let stream_tx = self.stream_tx.as_ref().unwrap().get();

                (stream_tx.round_sender_clone(), stream_tx.get_lock())
            };

            Box::pin(async move {
                let mut lock = lock.get_mut().await;
                let ret = sender.send(TunnelPack::TunnelData(tunnel_data)).await;
                *lock = true;
                ret
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
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        log::trace!(target: "main", "skip waning is_client:{}", self.is_client);
        let mut is_read = false;
        loop {
            if self.is_read_close() {
                return Poll::Ready(Ok(()));
            }

            if self.read_buf(buf) {
                is_read = true;
                if buf.initialize_unfilled().len() <= 0 {
                    return Poll::Ready(Ok(()));
                }
            }

            let mut stream_rx_future = if self.stream_rx_future.is_some() {
                self.stream_rx_future.take().unwrap()
            } else {
                match self.stream_rx.as_ref().unwrap().try_recv() {
                    Ok(tunnel_data) => {
                        self.stream_rx_pack_id
                            .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                        log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);
                        let DynamicTunnelData { header: _, datas } = tunnel_data;
                        self.stream_rx_buf.set_bytes(datas);
                        continue;
                    }
                    Err(async_channel::TryRecvError::Empty) => {}
                    Err(async_channel::TryRecvError::Closed) => {
                        self.read_close();
                        continue;
                    }
                }

                if is_read {
                    return Poll::Ready(Ok(()));
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
                Poll::Ready(Ok(tunnel_data)) => {
                    self.stream_rx_pack_id
                        .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                    log::trace!(target: "main", "read tunnel_data.header:{:?}", tunnel_data.header);
                    let DynamicTunnelData { header: _, datas } = tunnel_data;
                    self.stream_rx_buf.set_bytes(datas);
                    continue;
                }
                Poll::Pending => {
                    //Pending的时候保存起来
                    self.stream_rx_future = Some(stream_rx_future);
                    if is_read {
                        return Poll::Ready(Ok(()));
                    } else {
                        return Poll::Pending;
                    }
                }
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

            let stream_tx_pack_id = self.add_write_pack_id();

            let mut tunnel_data = TunnelDataVec::default();
            tunnel_data.datas.extend_from_slice(buf);
            tunnel_data.header.pack_id = stream_tx_pack_id;
            tunnel_data.header.pack_size = tunnel_data.datas.len() as u32;
            log::trace!(target: "main", "write tunnel_data.header:{:?}", tunnel_data.header);

            let (sender, lock) = {
                let stream_tx = self.stream_tx.as_ref().unwrap().get();

                (stream_tx.round_sender_clone(), stream_tx.get_lock())
            };

            Box::pin(async move {
                let mut lock = lock.get_mut().await;
                let ret = sender
                    .send(TunnelPack::TunnelData(DynamicTunnelData::new(tunnel_data)))
                    .await;
                *lock = true;
                ret
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
        // if self.is_write_close() {
        //     return Poll::Ready(Err(std::io::Error::new(
        //         std::io::ErrorKind::ConnectionReset,
        //         "ConnectionReset",
        //     )));
        // }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.write_close();
        Poll::Ready(Ok(()))
    }
}
