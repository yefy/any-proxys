use super::peer_stream::PeerStream;
use super::peer_stream_connect::PeerStreamConnect;
use super::protopack::DynamicTunnelData;
use super::protopack::TunnelAddConnect;
use super::protopack::TunnelHello;
use super::protopack::TunnelMaxConnect;
use super::protopack::TunnelPack;
use super::protopack::TUNNEL_VERSION;
use super::round_async_channel::RoundAsyncChannel;
use super::stream_flow::StreamFlow;
use crate::client::AsyncClient;
use crate::peer_stream::{PeerStreamKey, PeerStreamRecv, PeerStreamRecvHello};
use crate::server::{AcceptSenderType, ServerContext};
use crate::stream::Stream;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use crate::PeerClientToStreamSender;
#[cfg(feature = "anytunnel-debug")]
use crate::DEFAULT_PRINT_NUM;

pub struct PeerClient {
    is_client: bool,
    peer_stream_size: Arc<AtomicUsize>,
    wait_peer_stream_size: Arc<AtomicUsize>,
    session_id: Arc<Mutex<Option<String>>>,
    round_async_channel: Arc<Mutex<RoundAsyncChannel<TunnelPack>>>,
    peer_stream_to_peer_client_tx: async_channel::Sender<TunnelPack>,
    peer_client_to_stream_tx: PeerClientToStreamSender,
    peer_stream_to_peer_client_rx: async_channel::Receiver<TunnelPack>,
    peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
    is_peer_client_to_stream_timer: Arc<AtomicBool>,
    async_client: Option<Arc<AsyncClient>>,
    is_peer_stream_max: Arc<AtomicBool>,
    server_context: Option<Arc<Mutex<ServerContext>>>,
    peer_stream_index: AtomicUsize,
    channel_size: usize,
}

impl Drop for PeerClient {
    fn drop(&mut self) {
        self.close();
    }
}

impl PeerClient {
    pub fn close(&self) {
        #[cfg(feature = "anytunnel-debug")]
        log::info!("flag:{} peer_client close", get_flag(self.is_client));
        self.peer_stream_to_peer_client_rx.close();
        self.peer_client_to_stream_tx.close();
    }

    pub fn add_peer_stream_index(&self) -> usize {
        self.peer_stream_index.fetch_add(1, Ordering::SeqCst)
    }

    pub fn del_stream(&self) {
        if self.server_context.is_some() {
            #[cfg(feature = "anytunnel-debug")]
            log::info!("flag:{} peer_client del_stream", get_flag(self.is_client));

            let session_id = { self.session_id.lock().unwrap().clone().unwrap() };
            {
                self.server_context
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .delete(session_id);
            }
        }
    }

    pub fn peer_stream_size(&self) -> usize {
        return self.peer_stream_size.load(Ordering::SeqCst);
    }
    pub fn add_peer_stream_size(&self) {
        self.peer_stream_size.fetch_add(1, Ordering::SeqCst);
    }
    pub fn new(
        is_client: bool,
        peer_client_to_stream_tx: PeerClientToStreamSender,
        peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
        async_client: Option<Arc<AsyncClient>>,
        session_id: Option<String>,
        peer_stream_size: Option<Arc<AtomicUsize>>,
        server_context: Option<Arc<Mutex<ServerContext>>>,
        channel_size: usize,
    ) -> PeerClient {
        let round_async_channel = Arc::new(Mutex::new(RoundAsyncChannel::<TunnelPack>::new()));
        let (peer_stream_to_peer_client_tx, peer_stream_to_peer_client_rx) =
            async_channel::bounded(channel_size);
        let peer_stream_size = if peer_stream_size.is_some() {
            let peer_stream_size = peer_stream_size.unwrap();
            peer_stream_size.store(0, Ordering::SeqCst);
            peer_stream_size
        } else {
            Arc::new(AtomicUsize::new(0))
        };
        PeerClient {
            is_client,
            peer_stream_size,
            wait_peer_stream_size: Arc::new(AtomicUsize::new(0)),
            session_id: Arc::new(Mutex::new(session_id)),
            round_async_channel,
            peer_stream_to_peer_client_tx,
            peer_client_to_stream_tx,
            peer_stream_to_peer_client_rx,
            peer_stream_connect,
            is_peer_client_to_stream_timer: Arc::new(AtomicBool::new(false)),
            async_client,
            is_peer_stream_max: Arc::new(AtomicBool::new(false)),
            server_context,
            peer_stream_index: AtomicUsize::new(1),
            channel_size,
        }
    }
    pub async fn start(&self) -> Result<()> {
        let ret: Result<()> = async {
            tokio::select! {
                biased;
                ret = self.peer_client_to_stream() => {
                    ret.map_err(|e| anyhow!("err:peer_client_to_stream => e:{}", e))?;
                    Ok(())
                }
                ret = self.check_or_create_peer_stream() => {
                    ret.map_err(|e| anyhow!("err:create_connect => e:{}", e))?;
                    Ok(())
                }
                _ = self.check_peer_client_to_stream_timer() => {
                    Ok(())
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        }
        .await;
        self.del_stream();
        ret
    }

    async fn check_peer_client_to_stream_timer(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            self.is_peer_client_to_stream_timer
                .store(true, Ordering::Relaxed)
        }
    }

    async fn peer_client_to_stream(&self) -> Result<()> {
        log::trace!("skip waning is_client:{}", self.is_client);
        let mut pack_id = 1u32;
        let mut send_pack_id_map = HashMap::<u32, ()>::new();
        let mut recv_pack_cache_map = HashMap::<u32, DynamicTunnelData>::new();
        loop {
            if self.peer_client_to_stream_tx.is_closed() {
                #[cfg(feature = "anytunnel-debug")]
                log::info!(
                    "flag:{} peer_client self.peer_client_to_stream_tx.is_closed()",
                    get_flag(self.is_client)
                );
                return Ok(());
            }

            let tunnel_data = tokio::time::timeout(
                tokio::time::Duration::from_secs(1),
                self.peer_stream_to_peer_client_rx.recv(),
            )
            .await;
            let mut tunnel_data = match tunnel_data {
                Ok(tunnel_data) => {
                    if let Err(_) = tunnel_data {
                        #[cfg(feature = "anytunnel-debug")]
                        log::info!(
                            "flag:{} peer_client self.peer_stream_to_peer_client_rx.recv()",
                            get_flag(self.is_client)
                        );
                        return Ok(());
                    }
                    let tunnel_data = tunnel_data.unwrap();
                    match tunnel_data {
                        TunnelPack::TunnelHello(value) => {
                            log::error!("err:peer_client recv TunnelHello => value:{:?}", value);
                            return Ok(());
                        }
                        TunnelPack::TunnelClose(value) => {
                            log::error!("err:peer_client recv TunnelClose => value:{:?}", value);
                            return Ok(());
                        }
                        TunnelPack::TunnelData(tunnel_data) => {
                            if send_pack_id_map.get(&tunnel_data.header.pack_id).is_some() {
                                return Err(anyhow!(
                                    "err:pack_id exist => pack_id:{}",
                                    tunnel_data.header.pack_id
                                ))?;
                            }
                            if tunnel_data.header.pack_id == pack_id {
                                Some(tunnel_data)
                            } else {
                                recv_pack_cache_map.insert(tunnel_data.header.pack_id, tunnel_data);
                                None
                            }
                        }
                        TunnelPack::TunnelAddConnect(value) => {
                            #[cfg(feature = "anytunnel-debug")]
                            log::info!(
                                "flag:{} peer_client TunnelAddConnect:{:?}",
                                get_flag(self.is_client),
                                value
                            );
                            if self.peer_stream_connect.is_none() {
                                log::error!("err:peer_stream_connect.is_none value:{:?}", value);
                                return Ok(());
                            }

                            let peer_stream_connect = self.peer_stream_connect.as_ref().unwrap();
                            let peer_max_stream_size = peer_stream_connect.max_stream_size();
                            if self.peer_stream_size() >= peer_max_stream_size {
                                //TunnelMaxConnect
                                let tunnel_max_connect =
                                    TunnelPack::TunnelMaxConnect(TunnelMaxConnect {
                                        peer_stream_size: self.peer_stream_size(),
                                    });
                                let sender = {
                                    self.round_async_channel
                                        .lock()
                                        .unwrap()
                                        .round_sender_clone()
                                };
                                let _ = sender.send(tunnel_max_connect).await;
                            } else {
                                self.wait_peer_stream_size.fetch_add(1, Ordering::SeqCst);
                            }
                            None
                        }
                        TunnelPack::TunnelMaxConnect(value) => {
                            #[cfg(feature = "anytunnel-debug")]
                            log::info!(
                                "flag:{} peer_client TunnelMaxConnect:{:?}",
                                get_flag(self.is_client),
                                value
                            );
                            if self.peer_stream_connect.is_some() {
                                log::error!("err:peer_stream_connect.is_some => value:{:?}", value);
                                return Ok(());
                            }
                            self.is_peer_stream_max.store(true, Ordering::Relaxed);
                            None
                        }
                    }
                }
                Err(_) => None,
            };

            self.is_peer_client_to_stream_timer
                .store(false, Ordering::Relaxed);
            loop {
                let tunnel_data = if tunnel_data.is_some() {
                    tunnel_data.take().unwrap()
                } else {
                    if let Some(tunnel_data) = recv_pack_cache_map.remove(&pack_id) {
                        tunnel_data
                    } else {
                        break;
                    }
                };
                send_pack_id_map.insert(tunnel_data.header.pack_id, ());
                pack_id += 1;

                if let Err(_) = self.peer_client_to_stream_tx.send(tunnel_data).await {
                    log::debug!("peer_client_to_stream_tx close");
                    #[cfg(feature = "anytunnel-debug")]
                    log::info!(
                        "flag:{} peer_client self.peer_client_to_stream_tx.send",
                        get_flag(self.is_client)
                    );
                    return Ok(());
                }
                if self.is_peer_client_to_stream_timer.load(Ordering::Relaxed) {
                    break;
                }
            }

            #[cfg(feature = "anytunnel-debug")]
            {
                if pack_id % DEFAULT_PRINT_NUM == 0 {
                    log::info!(
                        "flag:{} peer_client pack_id:{} recv_pack_cache_map.len:{}",
                        get_flag(self.is_client),
                        pack_id,
                        recv_pack_cache_map.len()
                    );
                }
            }
        }
    }

    async fn check_or_create_peer_stream(&self) -> Result<()> {
        let mut w_num = 0;
        let max_num = 2;
        #[cfg(feature = "anytunnel-debug")]
        let mut debug_num = 0;
        loop {
            #[cfg(feature = "anytunnel-debug")]
            {
                debug_num += 1;
                if debug_num >= 16 {
                    debug_num = 0;
                    log::info!(
                        "flag:{}, peer_client peer_stream_size:{}",
                        get_flag(self.is_client),
                        self.peer_stream_size()
                    );
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            //有堵塞了才需要创建链接
            let mut is_w = {
                let is_full = { self.round_async_channel.lock().unwrap().is_full() };
                if is_full {
                    w_num += 1;
                    if w_num < max_num {
                        false
                    } else {
                        w_num = 0;
                        true
                    }
                } else {
                    w_num = 0;
                    false
                }
            };

            if !is_w {
                if self.wait_peer_stream_size.load(Ordering::SeqCst) > 0 {
                    self.wait_peer_stream_size.fetch_sub(1, Ordering::SeqCst);
                    is_w = true;
                }
            }

            if !is_w {
                continue;
            }

            if self.peer_stream_connect.is_none() {
                if !self.is_peer_stream_max.load(Ordering::Relaxed) {
                    //TunnelAddConnect
                    let (sender, lock) = {
                        let round_async_channel = self.round_async_channel.lock().unwrap();

                        (
                            round_async_channel.round_sender_clone(),
                            round_async_channel.get_lock(),
                        )
                    };

                    //这里不加锁，队列永远都是full的， 不知道哪里有问题
                    let mut lock = lock.lock().await;
                    let _ = sender
                        .send(TunnelPack::TunnelAddConnect(TunnelAddConnect {
                            peer_stream_size: 0,
                        }))
                        .await;
                    *lock = true;
                }
                continue;
            }

            let peer_stream_connect = self.peer_stream_connect.as_ref().unwrap();
            let peer_max_stream_size = peer_stream_connect.max_stream_size();
            if self.peer_stream_size() >= peer_max_stream_size {
                continue;
            }

            if let Err(e) = self
                .create_peer_stream(None)
                .await
                .map_err(|e| anyhow!("err:create_peer_stream => e:{}", e))
            {
                log::error!("{}", e);
            }
        }
    }

    pub async fn create_stream_and_peer_client(
        is_client: bool,
        max_stream_size: usize,
        client_info: Option<(i32, u32)>,
        async_client: Option<Arc<AsyncClient>>,
        peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
        peer_stream_key: Option<&Arc<PeerStreamKey>>,
        min_stream_cache_size: usize,
        peer_stream_size: Option<Arc<AtomicUsize>>,
        session_id: Option<String>,
        server_context: Option<Arc<Mutex<ServerContext>>>,
        channel_size: usize,
    ) -> Result<(Arc<PeerClient>, Stream, SocketAddr, SocketAddr)> {
        let (peer_client_to_stream_tx, peer_client_to_stream_rx) =
            async_channel::bounded(channel_size);
        let peer_client = Arc::new(PeerClient::new(
            is_client,
            peer_client_to_stream_tx,
            peer_stream_connect,
            async_client,
            session_id,
            peer_stream_size,
            server_context,
            channel_size,
        ));
        let round_async_channel = peer_client.round_async_channel.clone();

        let (local_addr, remote_addr) = if is_client {
            peer_client
                .create_peer_stream(client_info)
                .await
                .map_err(|e| {
                    anyhow!(
                        "err:create_peer_stream => is_client:{}, err:{}",
                        is_client,
                        e
                    )
                })?
        } else {
            let peer_stream_key = peer_stream_key.unwrap();
            peer_client
                .hello_to_peer_stream(None, peer_stream_key, min_stream_cache_size)
                .await
                .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e))?;
            (peer_stream_key.local_addr, peer_stream_key.remote_addr)
        };

        let peer_client_spawn = peer_client.clone();
        any_base::executor_spawn::_start_and_free(move || async move {
            peer_client_spawn
                .start()
                .await
                .map_err(|e| anyhow!("err:peer_client.start => e:{}", e))?;
            Ok(())
        });

        Ok((
            peer_client,
            Stream::new(
                is_client,
                peer_client_to_stream_rx,
                round_async_channel,
                max_stream_size,
                channel_size,
            ),
            local_addr,
            remote_addr,
        ))
    }

    pub async fn create_peer_stream(
        &self,
        client_info: Option<(i32, u32)>,
    ) -> Result<(SocketAddr, SocketAddr)> {
        if self.peer_stream_connect.is_none() {
            let err = anyhow!("err:peer_stream_connect.is_none");
            log::error!("{}", err);
            return Err(err);
        }
        self.add_peer_stream_size();
        let (connect_addr, min_stream_cache_size) = {
            let peer_stream_connect = self.peer_stream_connect.as_ref().unwrap();
            (
                peer_stream_connect.connect_addr()?,
                peer_stream_connect.min_stream_cache_size(),
            )
        };

        let peer_stream_key = self
            .async_client
            .as_ref()
            .unwrap()
            .get_peer_stream_key(&connect_addr.to_string(), min_stream_cache_size);
        let peer_stream_key = if peer_stream_key.is_some() {
            #[cfg(feature = "anytunnel-debug")]
            log::info!("flag:{} peer_client cache", get_flag(self.is_client));
            peer_stream_key.unwrap()
        } else {
            #[cfg(feature = "anytunnel-debug")]
            log::info!("flag:{} peer_client new connect", get_flag(self.is_client));
            let (stream, local_addr, remote_addr) = self
                .peer_stream_connect
                .as_ref()
                .unwrap()
                .connect(&connect_addr)
                .await
                .map_err(|e| anyhow!("err:connect => e:{}", e))?;

            PeerClient::do_create_peer_stream(
                true,
                local_addr,
                remote_addr,
                self.async_client.clone(),
                stream,
                None,
            )
            .await?
        };

        let session_id = {
            let mut session_id_ = self.session_id.lock().unwrap();
            if session_id_.is_some() {
                session_id_.clone().unwrap()
            } else {
                if client_info.is_none() {
                    let err = anyhow!("err:client_info.is_none");
                    log::error!("{}", err);
                    return Err(err);
                }
                let (pid, client_id) = client_info.unwrap();

                let session_id = format!(
                    "{}{:?}{}{}{}{}",
                    pid,
                    std::thread::current().id(),
                    client_id,
                    peer_stream_key.local_addr,
                    peer_stream_key.remote_addr,
                    Local::now().timestamp_millis(),
                );
                *session_id_ = Some(session_id.clone());
                session_id
            }
        };
        let tunnel_hello = TunnelHello {
            version: TUNNEL_VERSION.to_string(),
            session_id,
            min_stream_cache_size,
            channel_size: self.channel_size,
        };
        #[cfg(feature = "anytunnel-debug")]
        {
            log::info!("tunnel_hello flag:client, tunnel_hello:{:?}", tunnel_hello);
        }
        self.hello_to_peer_stream(Some(tunnel_hello), &peer_stream_key, min_stream_cache_size)
            .await
            .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e))?;

        Ok((peer_stream_key.local_addr, peer_stream_key.remote_addr))
    }

    pub async fn do_create_peer_stream(
        is_client: bool,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        async_client: Option<Arc<AsyncClient>>,
        stream: StreamFlow,
        accept_tx: Option<AcceptSenderType>,
    ) -> Result<Arc<PeerStreamKey>> {
        let peer_stream =
            PeerStream::new(is_client, local_addr, remote_addr, async_client, accept_tx);
        let peer_stream_key = peer_stream.peer_stream_key();
        PeerStream::start(peer_stream, stream).await?;
        Ok(peer_stream_key)
    }

    pub async fn hello_to_peer_stream(
        &self,
        tunnel_hello: Option<TunnelHello>,
        peer_stream_key: &PeerStreamKey,
        min_stream_cache_size: usize,
    ) -> Result<()> {
        let stream_to_peer_stream_rx = {
            self.round_async_channel
                .lock()
                .unwrap()
                .channel(self.channel_size)
        };

        #[cfg(feature = "anytunnel-debug")]
        {
            log::info!(
                "hello flag:{}, min_stream_cache_size:{}",
                if tunnel_hello.is_some() {
                    "client"
                } else {
                    "server"
                },
                min_stream_cache_size
            );
        }

        let hello = PeerStreamRecvHello {
            stream_to_peer_stream_rx,
            peer_stream_to_peer_client_tx: self.peer_stream_to_peer_client_tx.clone(),
            tunnel_hello,
            min_stream_cache_size,
            peer_stream_index: self.add_peer_stream_index(),
            channel_size: self.channel_size,
        };

        peer_stream_key
            .to_peer_stream_tx
            .send(PeerStreamRecv::PeerStreamRecvHello(hello))
            .await?;
        Ok(())
    }
}
