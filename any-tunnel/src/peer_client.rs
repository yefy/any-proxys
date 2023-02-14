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
use crate::client::ClientContext;
use crate::peer_stream::{PeerStreamKey, PeerStreamRecv, PeerStreamRecvHello};
use crate::server::{AcceptSenderType, ServerContext};
use crate::stream::Stream;
use crate::PeerClientToStreamSender;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::{WaitGroup, WorkerInner};
use chrono::Local;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use lazy_static::lazy_static;
lazy_static! {
    static ref PEER_CLIENT_NUM: AtomicU32 = AtomicU32::new(0);
}

pub struct PeerStreamToPeerClientTx {
    pub ref_count: Arc<AtomicUsize>,
    pub tx: async_channel::Sender<TunnelPack>,
    pub wait_group: WaitGroup,
    pub worker_inner: Option<WorkerInner>,
}

impl PeerStreamToPeerClientTx {
    pub fn new(tx: async_channel::Sender<TunnelPack>) -> PeerStreamToPeerClientTx {
        PeerStreamToPeerClientTx {
            ref_count: Arc::new(AtomicUsize::new(0)),
            tx,
            wait_group: WaitGroup::new(),
            worker_inner: None,
        }
    }

    pub fn clone_and_ref(&self) -> PeerStreamToPeerClientTx {
        let worker_inner = self.wait_group.worker().add();
        self.ref_count.fetch_add(1, Ordering::Relaxed);
        PeerStreamToPeerClientTx {
            ref_count: self.ref_count.clone(),
            tx: self.tx.clone(),
            wait_group: self.wait_group.clone(),
            worker_inner: Some(worker_inner),
        }
    }

    pub fn close(&self) {
        self.tx.close();
    }
}

impl Drop for PeerStreamToPeerClientTx {
    fn drop(&mut self) {
        if self.worker_inner.is_some() {
            self.worker_inner.as_ref().unwrap().done();
        }
        if self.ref_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.tx.close();
        }
    }
}

pub struct PeerClientContext {
    is_client: bool,
    peer_client_to_stream_tx: PeerClientToStreamSender,
    peer_client_max_pack_id: Arc<AtomicU32>,
    is_peer_client_to_stream_timer: Arc<AtomicBool>,
    peer_stream_to_peer_client_tx: PeerStreamToPeerClientTx,
    stream_wait_group: Arc<WaitGroup>,
    peer_stream_to_peer_client_rx: async_channel::Receiver<TunnelPack>,
    session_id: Arc<Mutex<Option<String>>>,
    peer_client_order_pack_id: Arc<AtomicU32>,
    peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
    peer_stream_size: Arc<AtomicUsize>,
    round_async_channel: Arc<Mutex<RoundAsyncChannel<TunnelPack>>>,
    wait_peer_stream_size: Arc<AtomicUsize>,
    is_peer_stream_max: Arc<AtomicBool>,
}

impl PeerClientContext {
    pub fn peer_stream_size(&self) -> usize {
        return self.peer_stream_size.load(Ordering::SeqCst);
    }

    pub fn add_peer_stream_size(&self) {
        self.peer_stream_size.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct PeerClient {
    client_context: Option<Arc<ClientContext>>,
    server_context: Option<Arc<Mutex<ServerContext>>>,
    peer_stream_index: AtomicUsize,
    channel_size: usize,
    stream_tx_pack_id: Arc<AtomicU32>,
    stream_rx_pack_id: Arc<AtomicU32>,
    peer_stream_tx_pack_id: Arc<AtomicU32>,
    peer_stream_rx_pack_id: Arc<AtomicU32>,
    is_error: Arc<AtomicBool>,
    context: Arc<PeerClientContext>,
}

impl Drop for PeerClient {
    fn drop(&mut self) {
        self.close();
    }
}

impl PeerClient {
    pub fn new(
        is_client: bool,
        peer_client_to_stream_tx: PeerClientToStreamSender,
        peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
        client_context: Option<Arc<ClientContext>>,
        session_id: Option<String>,
        peer_stream_size: Option<Arc<AtomicUsize>>,
        server_context: Option<Arc<Mutex<ServerContext>>>,
        channel_size: usize,
    ) -> PeerClient {
        #[cfg(feature = "anydebug")]
        {
            let num = PEER_CLIENT_NUM.fetch_add(1, Ordering::Relaxed) + 1;
            log::info!("new PEER_CLIENT_NUM:{}", num);
        }
        let round_async_channel = Arc::new(Mutex::new(RoundAsyncChannel::<TunnelPack>::new()));
        let (peer_stream_to_peer_client_tx, peer_stream_to_peer_client_rx) =
            async_channel::bounded(channel_size);
        let peer_stream_to_peer_client_tx =
            PeerStreamToPeerClientTx::new(peer_stream_to_peer_client_tx);
        let peer_stream_size = if peer_stream_size.is_some() {
            let peer_stream_size = peer_stream_size.unwrap();
            peer_stream_size.store(0, Ordering::SeqCst);
            peer_stream_size
        } else {
            Arc::new(AtomicUsize::new(0))
        };
        PeerClient {
            client_context,
            server_context,
            peer_stream_index: AtomicUsize::new(0),
            channel_size,
            stream_tx_pack_id: Arc::new(AtomicU32::new(0)),
            stream_rx_pack_id: Arc::new(AtomicU32::new(0)),
            peer_stream_tx_pack_id: Arc::new(AtomicU32::new(0)),
            peer_stream_rx_pack_id: Arc::new(AtomicU32::new(0)),
            is_error: Arc::new(AtomicBool::new(false)),
            context: Arc::new(PeerClientContext {
                is_client,
                peer_client_to_stream_tx,
                peer_client_max_pack_id: Arc::new(AtomicU32::new(0)),
                is_peer_client_to_stream_timer: Arc::new(AtomicBool::new(false)),
                peer_stream_to_peer_client_tx,
                stream_wait_group: Arc::new(WaitGroup::new()),
                peer_stream_to_peer_client_rx,
                session_id: Arc::new(Mutex::new(session_id)),
                peer_client_order_pack_id: Arc::new(AtomicU32::new(0)),
                peer_stream_connect,
                peer_stream_size,
                round_async_channel,
                wait_peer_stream_size: Arc::new(AtomicUsize::new(0)),
                is_peer_stream_max: Arc::new(AtomicBool::new(false)),
            }),
        }
    }
    pub fn close(&self) {
        self.context.peer_stream_to_peer_client_rx.close();
        self.context.peer_client_to_stream_tx.close();
        #[cfg(feature = "anydebug")]
        {
            use crate::get_flag;
            let num = PEER_CLIENT_NUM.fetch_sub(1, Ordering::Relaxed) - 1;
            log::info!("del PEER_CLIENT_NUM:{}", num);
            log::info!(
                "session_id:{:?}, flag:{}, peer_stream_index:{}, peer_client close",
                self.context.session_id.lock().unwrap(),
                get_flag(self.context.is_client),
                self.peer_stream_index.load(Ordering::Relaxed),
            );
        }

        #[cfg(feature = "anyerror")]
        {
            use crate::get_flag;
            let mut is_error = false;
            let stream_tx_pack_id = self.stream_tx_pack_id.load(Ordering::SeqCst);
            let stream_rx_pack_id = self.stream_rx_pack_id.load(Ordering::SeqCst);
            let peer_stream_tx_pack_id = self.peer_stream_tx_pack_id.load(Ordering::SeqCst);
            let peer_stream_rx_pack_id = self.peer_stream_rx_pack_id.load(Ordering::SeqCst);
            let peer_client_order_pack_id = self
                .context
                .peer_client_order_pack_id
                .load(Ordering::SeqCst);
            let peer_client_max_pack_id =
                self.context.peer_client_max_pack_id.load(Ordering::SeqCst);

            if stream_tx_pack_id != peer_stream_tx_pack_id {
                is_error = true;
            }

            if stream_rx_pack_id != peer_stream_rx_pack_id {
                is_error = true;
            }

            if peer_client_max_pack_id != peer_client_order_pack_id {
                is_error = true;
            }

            if peer_client_order_pack_id != peer_stream_rx_pack_id {
                is_error = true;
            }

            if !is_error {
                is_error = self.is_error.load(Ordering::Relaxed);
            }

            if is_error {
                log::error!(
                    "session_id:{}, flag:{}, \
                stream_tx_pack_id:{}, stream_rx_pack_id:{}, \
           peer_stream_tx_pack_id:{}, peer_stream_rx_pack_id:{}, \
            peer_client_order_pack_id:{},  peer_client_max_pack_id:{}",
                    self.context.session_id.lock().unwrap().as_ref().unwrap(),
                    get_flag(self.context.is_client),
                    stream_tx_pack_id,
                    stream_rx_pack_id,
                    peer_stream_tx_pack_id,
                    peer_stream_rx_pack_id,
                    peer_client_order_pack_id,
                    peer_client_max_pack_id,
                )
            }
        }
    }

    pub fn add_peer_stream_index(&self) -> usize {
        self.peer_stream_index.fetch_add(1, Ordering::SeqCst)
    }

    pub fn del_stream(&self) {
        if self.server_context.is_none() {
            return;
        }
        let session_id = { self.context.session_id.lock().unwrap().clone().unwrap() };
        #[cfg(feature = "anydebug")]
        {
            use crate::get_flag;
            log::info!(
                "session_id:{}, flag:{} peer_client del",
                get_flag(self.context.is_client),
                session_id
            );
        }

        self.server_context
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .delete(session_id);
    }

    pub async fn start(&self) -> Result<()> {
        let mut peer_client_to_stream = PeerClientToStream::new(self.context.clone());
        let ret: Result<()> = async {
            tokio::select! {
                biased;
                ret = peer_client_to_stream.start() => {
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
            self.context
                .is_peer_client_to_stream_timer
                .store(true, Ordering::Relaxed)
        }
    }

    async fn check_or_create_peer_stream(&self) -> Result<()> {
        let mut w_num = 0;
        let max_num = 2;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            //有堵塞了才需要创建链接
            let mut is_w = {
                let is_full = { self.context.round_async_channel.lock().unwrap().is_full() };
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
                if self.context.wait_peer_stream_size.load(Ordering::SeqCst) > 0 {
                    self.context
                        .wait_peer_stream_size
                        .fetch_sub(1, Ordering::SeqCst);
                    is_w = true;
                }
            }

            if !is_w {
                continue;
            }

            if self.context.peer_stream_connect.is_none() {
                if !self.context.is_peer_stream_max.load(Ordering::Relaxed) {
                    if self.context.round_async_channel.lock().unwrap().is_close() {
                        continue;
                    }
                    //TunnelAddConnect
                    let (sender, lock) = {
                        let round_async_channel = self.context.round_async_channel.lock().unwrap();

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

            let peer_stream_connect = self.context.peer_stream_connect.as_ref().unwrap();
            let peer_max_stream_size = peer_stream_connect.max_stream_size();
            if self.context.peer_stream_size() >= peer_max_stream_size {
                continue;
            }
            if self.context.round_async_channel.lock().unwrap().is_close() {
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
        client_context: Option<Arc<ClientContext>>,
        peer_stream_connect: Option<Arc<Box<dyn PeerStreamConnect>>>,
        peer_stream_key: Option<&Arc<PeerStreamKey>>,
        min_stream_cache_size: usize,
        peer_stream_size: Option<Arc<AtomicUsize>>,
        session_id: Option<String>,
        server_context: Option<Arc<Mutex<ServerContext>>>,
        channel_size: usize,
        peer_stream_id: Option<usize>,
    ) -> Result<(Arc<PeerClient>, Stream, SocketAddr, SocketAddr)> {
        let (peer_client_to_stream_tx, peer_client_to_stream_rx) =
            async_channel::bounded(channel_size);
        let peer_client = Arc::new(PeerClient::new(
            is_client,
            peer_client_to_stream_tx,
            peer_stream_connect,
            client_context,
            session_id.clone(),
            peer_stream_size,
            server_context,
            channel_size,
        ));
        let round_async_channel = peer_client.context.round_async_channel.clone();

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
                .hello_to_peer_stream(
                    None,
                    peer_stream_key,
                    min_stream_cache_size,
                    session_id,
                    peer_stream_id,
                )
                .await
                .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e))?;
            (peer_stream_key.local_addr, peer_stream_key.remote_addr)
        };

        let peer_client_spawn = peer_client.clone();
        let async_ = Box::pin(async move {
            peer_client_spawn
                .start()
                .await
                .map_err(|e| anyhow!("err:peer_client.start => e:{}", e))
        });
        if cfg!(feature = "anyruntime-tokio-spawn-local") {
            any_base::executor_local_spawn::_start_and_free2(move || async move { async_.await });
        } else {
            any_base::executor_spawn::_start_and_free(move || async move { async_.await });
        }
        let session_id = peer_client
            .context
            .session_id
            .lock()
            .unwrap()
            .clone()
            .unwrap();
        let stream_tx_pack_id = peer_client.stream_tx_pack_id.clone();
        let stream_rx_pack_id = peer_client.stream_rx_pack_id.clone();
        let worker_inner = peer_client.context.stream_wait_group.worker().add();
        Ok((
            peer_client,
            Stream::new(
                is_client,
                peer_client_to_stream_rx,
                round_async_channel,
                max_stream_size,
                channel_size,
                session_id,
                stream_tx_pack_id,
                stream_rx_pack_id,
                worker_inner,
            ),
            local_addr,
            remote_addr,
        ))
    }

    pub async fn create_peer_stream(
        &self,
        client_info: Option<(i32, u32)>,
    ) -> Result<(SocketAddr, SocketAddr)> {
        if self.context.peer_stream_connect.is_none() {
            let err = anyhow!("err:peer_stream_connect.is_none");
            log::error!("{}", err);
            return Err(err);
        }
        self.context.add_peer_stream_size();
        let (key, min_stream_cache_size) = {
            let peer_stream_connect = self.context.peer_stream_connect.as_ref().unwrap();
            (
                peer_stream_connect.key()?,
                peer_stream_connect.min_stream_cache_size(),
            )
        };

        let peer_stream_key = self
            .client_context
            .as_ref()
            .unwrap()
            .get_peer_stream_key(&key, min_stream_cache_size);

        let peer_stream_key = if peer_stream_key.is_some() {
            peer_stream_key.unwrap()
        } else {
            let (stream, local_addr, remote_addr) = self
                .context
                .peer_stream_connect
                .as_ref()
                .unwrap()
                .connect()
                .await
                .map_err(|e| anyhow!("err:connect => e:{}", e))?;

            let key = self.context.peer_stream_connect.as_ref().unwrap().key()?;
            PeerClient::do_create_peer_stream(
                true,
                key,
                local_addr,
                remote_addr,
                self.client_context.clone(),
                stream,
                None,
            )
            .await?
        };

        let session_id = {
            let mut session_id_ = self.context.session_id.lock().unwrap();
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
            session_id: session_id.clone(),
            min_stream_cache_size,
            channel_size: self.channel_size,
            client_peer_stream_index: 0,
        };

        self.hello_to_peer_stream(
            Some(tunnel_hello),
            &peer_stream_key,
            min_stream_cache_size,
            Some(session_id),
            None,
        )
        .await
        .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e))?;

        Ok((peer_stream_key.local_addr, peer_stream_key.remote_addr))
    }

    pub async fn do_create_peer_stream(
        is_client: bool,
        key: String,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        client_context: Option<Arc<ClientContext>>,
        stream: StreamFlow,
        accept_tx: Option<AcceptSenderType>,
    ) -> Result<Arc<PeerStreamKey>> {
        let peer_stream = PeerStream::new(
            is_client,
            local_addr,
            remote_addr,
            client_context,
            accept_tx,
            key,
        );
        let peer_stream_key = peer_stream.peer_stream_key();
        PeerStream::start(peer_stream, stream).await?;
        Ok(peer_stream_key)
    }

    pub async fn hello_to_peer_stream(
        &self,
        mut tunnel_hello: Option<TunnelHello>,
        peer_stream_key: &PeerStreamKey,
        min_stream_cache_size: usize,
        session_id: Option<String>,
        peer_stream_id: Option<usize>,
    ) -> Result<()> {
        let stream_to_peer_stream_rx = {
            self.context
                .round_async_channel
                .lock()
                .unwrap()
                .channel(self.channel_size)?
        };

        let peer_stream_index = self.add_peer_stream_index();
        if tunnel_hello.is_some() {
            tunnel_hello.as_mut().unwrap().client_peer_stream_index = peer_stream_index;
        }
        let hello = PeerStreamRecvHello {
            stream_to_peer_stream_rx,
            peer_stream_to_peer_client_tx: self
                .context
                .peer_stream_to_peer_client_tx
                .clone_and_ref(),
            tunnel_hello,
            min_stream_cache_size,
            peer_stream_index,
            channel_size: self.channel_size,
            peer_stream_tx_pack_id: self.peer_stream_tx_pack_id.clone(),
            peer_stream_rx_pack_id: self.peer_stream_rx_pack_id.clone(),
            session_id,
            is_error: self.is_error.clone(),
            peer_stream_id,
        };

        peer_stream_key
            .to_peer_stream_tx
            .send(PeerStreamRecv::PeerStreamRecvHello(hello))
            .await?;
        Ok(())
    }
}

struct PeerClientToStream {
    pack_id: u32,
    send_pack_id_map: HashMap<u32, ()>,
    recv_pack_cache_map: HashMap<u32, DynamicTunnelData>,
    is_close: bool,
    context: Arc<PeerClientContext>,
}

impl PeerClientToStream {
    pub fn new(context: Arc<PeerClientContext>) -> PeerClientToStream {
        PeerClientToStream {
            pack_id: 1u32,
            send_pack_id_map: HashMap::<u32, ()>::new(),
            recv_pack_cache_map: HashMap::<u32, DynamicTunnelData>::new(),
            is_close: false,
            context,
        }
    }

    async fn read_tunnel_data(&mut self) -> Result<Option<DynamicTunnelData>> {
        let tunnel_data = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            self.context.peer_stream_to_peer_client_rx.recv(),
        )
        .await;

        match tunnel_data {
            Ok(tunnel_data) => {
                match tunnel_data {
                    Err(_) => {
                        self.is_close = true;
                        Ok(None)
                    }
                    Ok(tunnel_data) => {
                        match tunnel_data {
                            TunnelPack::TunnelHello(value) => {
                                return Err(anyhow!(
                                    "err:peer_client recv TunnelHello => value:{:?}",
                                    value
                                ));
                            }
                            TunnelPack::TunnelClose(value) => {
                                return Err(anyhow!(
                                    "err:peer_client recv TunnelClose => value:{:?}",
                                    value
                                ));
                            }
                            TunnelPack::TunnelData(tunnel_data) => {
                                if self
                                    .send_pack_id_map
                                    .get(&tunnel_data.header.pack_id)
                                    .is_some()
                                {
                                    return Err(anyhow!(
                                        "err:pack_id exist => pack_id:{}",
                                        tunnel_data.header.pack_id
                                    ))?;
                                }
                                if tunnel_data.header.pack_id == self.pack_id {
                                    if self.recv_pack_cache_map.len() <= 0 {
                                        self.context
                                            .peer_client_max_pack_id
                                            .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                                    }
                                    Ok(Some(tunnel_data))
                                } else {
                                    self.context
                                        .peer_client_max_pack_id
                                        .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                                    self.recv_pack_cache_map
                                        .insert(tunnel_data.header.pack_id, tunnel_data);
                                    Ok(None)
                                }
                            }
                            TunnelPack::TunnelAddConnect(value) => {
                                if self.context.peer_stream_connect.is_none() {
                                    return Err(anyhow!(
                                        "err:peer_stream_connect.is_none value:{:?}",
                                        value
                                    ));
                                }

                                let peer_stream_connect =
                                    self.context.peer_stream_connect.as_ref().unwrap();
                                let peer_max_stream_size = peer_stream_connect.max_stream_size();
                                if self.context.peer_stream_size() >= peer_max_stream_size {
                                    //TunnelMaxConnect
                                    let tunnel_max_connect =
                                        TunnelPack::TunnelMaxConnect(TunnelMaxConnect {
                                            peer_stream_size: self.context.peer_stream_size(),
                                        });
                                    let sender = {
                                        self.context
                                            .round_async_channel
                                            .lock()
                                            .unwrap()
                                            .round_sender_clone()
                                    };
                                    let _ = sender.send(tunnel_max_connect).await;
                                } else {
                                    self.context
                                        .wait_peer_stream_size
                                        .fetch_add(1, Ordering::SeqCst);
                                }
                                Ok(None)
                            }
                            TunnelPack::TunnelMaxConnect(value) => {
                                if self.context.peer_stream_connect.is_some() {
                                    return Err(anyhow!(
                                        "err:peer_stream_connect.is_some => value:{:?}",
                                        value
                                    ));
                                }
                                self.context
                                    .is_peer_stream_max
                                    .store(true, Ordering::Relaxed);
                                Ok(None)
                            }
                        }
                    }
                }
            }
            Err(_) => Ok(None),
        }
    }
    async fn start(&mut self) -> Result<()> {
        log::trace!("skip waning is_client:{}", self.context.is_client);
        loop {
            if self.is_close {
                self.context.peer_client_to_stream_tx.close();
                self.context
                    .peer_stream_to_peer_client_tx
                    .wait_group
                    .wait()
                    .await?;
                self.context.stream_wait_group.wait().await?;
                return Ok(());
            }

            if self.context.peer_client_to_stream_tx.is_closed() {
                self.context.peer_stream_to_peer_client_rx.close();
                self.context.stream_wait_group.wait().await?;
                self.context
                    .peer_stream_to_peer_client_tx
                    .wait_group
                    .wait()
                    .await?;
                return Ok(());
            }

            let mut tunnel_data = self.read_tunnel_data().await?;

            #[cfg(feature = "anydebug")]
            if self.is_close {
                use crate::get_flag;
                log::info!(
                    "session_id:{}, flag:{} is_close = true, write all pack id",
                    self.context.session_id.lock().unwrap().as_ref().unwrap(),
                    get_flag(self.context.is_client),
                )
            }

            self.context
                .is_peer_client_to_stream_timer
                .store(false, Ordering::Relaxed);
            loop {
                let tunnel_data = if tunnel_data.is_some() {
                    tunnel_data.take().unwrap()
                } else {
                    if let Some(tunnel_data) = self.recv_pack_cache_map.remove(&self.pack_id) {
                        tunnel_data
                    } else {
                        break;
                    }
                };

                self.context
                    .peer_client_order_pack_id
                    .store(tunnel_data.header.pack_id, Ordering::SeqCst);
                self.send_pack_id_map.insert(tunnel_data.header.pack_id, ());
                self.pack_id += 1;

                if let Err(_) = self
                    .context
                    .peer_client_to_stream_tx
                    .send(tunnel_data)
                    .await
                {
                    log::debug!("peer_client_to_stream_tx close");
                    return Ok(());
                }
                if !self.is_close {
                    if self
                        .context
                        .is_peer_client_to_stream_timer
                        .load(Ordering::Relaxed)
                    {
                        break;
                    }
                }
            }
        }
    }
}
