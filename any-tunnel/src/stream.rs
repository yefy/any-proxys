use super::protopack::DynamicTunnelData;
#[cfg(feature = "anytunnel-dynamic-pool")]
use super::protopack::TunnelData;
use super::protopack::TunnelPack;
use super::round_async_channel::RoundAsyncChannel;
use crate::protopack::DynamicPoolTunnelData;
#[cfg(feature = "anytunnel-dynamic-pool")]
use dynamic_pool::DynamicPool;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};

use crate::PeerClientToStreamReceiver;
#[cfg(feature = "anytunnel-debug")]
use crate::DEFAULT_PRINT_NUM;

struct StreamBuf {
    tunnel_data: DynamicTunnelData,
    pos: usize,
}

impl StreamBuf {
    fn new(tunnel_data: DynamicTunnelData) -> StreamBuf {
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
    is_client: bool,
    stream_rx: Option<PeerClientToStreamReceiver>,
    stream_rx_buf: Option<StreamBuf>,
    stream_rx_future: Option<
        Pin<
            Box<
                dyn Future<
                        Output = std::result::Result<DynamicTunnelData, async_channel::RecvError>,
                    > + std::marker::Send,
            >,
        >,
    >,
    stream_rx_pack_id: u32,

    stream_tx: Option<Arc<Mutex<RoundAsyncChannel<TunnelPack>>>>,
    stream_tx_data_size: usize,
    stream_tx_buffer_pool: DynamicPoolTunnelData,
    stream_tx_future: Option<
        Pin<
            Box<
                dyn Future<Output = std::result::Result<(), async_channel::SendError<TunnelPack>>>
                    + std::marker::Send,
            >,
        >,
    >,
    stream_tx_pack_id: u32,
}

impl Stream {
    pub fn new(
        is_client: bool,
        stream_rx: PeerClientToStreamReceiver,
        stream_tx: Arc<Mutex<RoundAsyncChannel<TunnelPack>>>,
        _max_stream_size: usize,
        _channel_size: usize,
    ) -> Stream {
        #[cfg(feature = "anytunnel-dynamic-pool")]
        let stream_tx_buffer_pool =
            DynamicPool::new(8, _max_stream_size * _channel_size * 2, TunnelData::default);
        #[cfg(not(feature = "anytunnel-dynamic-pool"))]
        let stream_tx_buffer_pool = DynamicPoolTunnelData::new();
        Stream {
            is_client,
            stream_rx: Some(stream_rx),
            stream_rx_buf: None,
            stream_rx_future: None,
            stream_tx_future: None,
            stream_tx_data_size: 0,
            stream_tx_pack_id: 0,
            stream_rx_pack_id: 0,
            stream_tx: Some(stream_tx),
            stream_tx_buffer_pool,
        }
    }

    fn add_write_pack_id(&mut self) -> u32 {
        self.stream_tx_pack_id += 1;
        self.stream_tx_pack_id
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
            let stream_rx = stream_rx.unwrap();
            stream_rx.close();
            #[cfg(feature = "anytunnel-debug")]
            log::info!(
                "flag:{} stream stream_tx_pack_id:{}, stream_rx_pack_id:{}",
                get_flag(self.is_client),
                self.stream_tx_pack_id,
                self.stream_rx_pack_id
            );
            #[cfg(feature = "anytunnel-debug")]
            log::info!("flag:{} stream read_close", get_flag(self.is_client));
        }
    }

    pub fn write_close(&mut self) {
        let stream_tx = self.stream_tx.take();
        if stream_tx.is_some() {
            log::debug!("stream write_close");
            let stream_tx = stream_tx.unwrap();
            let stream_tx = &mut *stream_tx.lock().unwrap();
            stream_tx.close();
            #[cfg(feature = "anytunnel-debug")]
            log::info!("flag:{} stream write_close", get_flag(self.is_client));
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
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.close()
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.is_read_close() {
            log::error!("err:is_read_close");
            return Poll::Ready(Ok(()));
        }

        log::trace!("skip waning is_client:{}", self.is_client);
        if self.read_buf(buf) {
            return Poll::Ready(Ok(()));
        }

        let mut stream_rx_future = if self.stream_rx_future.is_some() {
            self.stream_rx_future.take().unwrap()
        } else {
            let stream_rx = self.stream_rx.clone().unwrap();
            Box::pin(async move { stream_rx.recv().await })
        };

        let ret = stream_rx_future.as_mut().poll(_cx);
        match ret {
            Poll::Ready(Err(_)) => {
                self.read_close();
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(Ok(tunnel_data)) => {
                self.stream_rx_pack_id = tunnel_data.header.pack_id;
                log::trace!("read tunnel_data.header:{:?}", tunnel_data.header);
                #[cfg(feature = "anytunnel-debug")]
                {
                    if self.stream_rx_pack_id % DEFAULT_PRINT_NUM == 0 {
                        log::info!(
                            "flag:{} stream read pack_id:{}",
                            get_flag(self.is_client),
                            self.stream_rx_pack_id
                        );
                    }
                }

                self.stream_rx_buf = Some(StreamBuf::new(tunnel_data));

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
            log::error!("err:is_write_close");
            return Poll::Ready(Ok(0));
        }

        let mut stream_tx_future = if self.stream_tx_future.is_some() {
            self.stream_tx_future.take().unwrap()
        } else {
            self.stream_tx_data_size = buf.len();

            let stream_tx_pack_id = self.add_write_pack_id();
            #[cfg(feature = "anytunnel-debug")]
            {
                if stream_tx_pack_id % DEFAULT_PRINT_NUM == 0 {
                    log::info!(
                        "flag:{} stream write pack_id:{}",
                        get_flag(self.is_client),
                        stream_tx_pack_id
                    );
                }
            }

            let mut tunnel_data = self.stream_tx_buffer_pool.take();
            tunnel_data.datas.extend_from_slice(buf);
            tunnel_data.header.pack_id = stream_tx_pack_id;
            tunnel_data.header.pack_size = tunnel_data.datas.len() as u32;
            log::trace!("write tunnel_data.header:{:?}", tunnel_data.header);

            let (sender, lock) = {
                let stream_tx = self.stream_tx.as_ref().unwrap().lock().unwrap();

                (stream_tx.round_sender_clone(), stream_tx.get_lock())
            };

            Box::pin(async move {
                let mut lock = lock.lock().await;
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
