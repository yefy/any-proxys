use super::protopack;
#[cfg(not(feature = "anypool-dynamic-pool"))]
use super::protopack::DynamicPoolTunnelData;
#[cfg(feature = "anypool-dynamic-pool")]
use super::protopack::TunnelData;
use super::protopack::TunnelDynamicPool;
use super::protopack::TunnelHeaderType;
use super::protopack::TunnelPack;
use super::server::ServerRecv;
use super::stream_flow::StreamFlow;
use super::stream_flow::StreamFlowErr;
use super::stream_flow::StreamFlowInfo;
#[cfg(feature = "anypool-dynamic-pool")]
use crate::client::ClientContext;
use crate::get_flag;
use crate::peer_client::PeerStreamToPeerClientTx;
use crate::protopack::{TunnelClose, TunnelHello};
use crate::server::{AcceptSenderType, ServerRecvHello};
use any_base::executor_local_spawn::Runtime;
use any_base::typ::ArcMutex;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
#[cfg(feature = "anypool-dynamic-pool")]
use dynamic_pool::DynamicPool;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

pub struct PeerStreamKey {
    pub key: String,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub domain: Option<ArcString>,
    pub to_peer_stream_tx: async_channel::Sender<PeerStreamRecv>,
}

pub struct PeerStreamRecvHello {
    pub stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
    pub peer_stream_to_peer_client_tx: PeerStreamToPeerClientTx,
    pub tunnel_hello: Option<TunnelHello>,
    pub min_stream_cache_size: usize,
    pub peer_stream_index: usize,
    pub channel_size: usize,
    pub peer_stream_tx_pack_id: Arc<AtomicU32>,
    pub peer_stream_rx_pack_id: Arc<AtomicU32>,
    pub session_id: Option<ArcString>,
    pub is_error: Arc<AtomicBool>,
    pub peer_stream_id: Option<usize>,
}

pub enum PeerStreamRecv {
    PeerStreamRecvHello(PeerStreamRecvHello),
}

pub struct PeerStreamOnce {
    close_num: AtomicUsize,
    tunnel_peer_close: Mutex<Option<TunnelClose>>,
    is_send_close: AtomicBool,
    session_id: Mutex<Option<ArcString>>,
    peer_stream_index: AtomicUsize,
    peer_stream_id: AtomicUsize,
    tx_pack_id: AtomicU32,
    rx_pack_id: AtomicU32,
}

impl PeerStreamOnce {
    pub fn new() -> PeerStreamOnce {
        PeerStreamOnce {
            close_num: AtomicUsize::new(0),
            tunnel_peer_close: Mutex::new(None),
            is_send_close: AtomicBool::new(false),
            session_id: Mutex::new(None),
            peer_stream_index: AtomicUsize::new(0),
            peer_stream_id: AtomicUsize::new(0),
            tx_pack_id: AtomicU32::new(0),
            rx_pack_id: AtomicU32::new(0),
        }
    }

    pub fn init(&self) {
        self.close_num.store(0, Ordering::SeqCst);
        {
            self.tunnel_peer_close.lock().unwrap().take();
        }
        self.is_send_close.store(false, Ordering::SeqCst);
        {
            *self.session_id.lock().unwrap() = None;
        }
        self.peer_stream_index.store(0, Ordering::Relaxed);
        self.peer_stream_id.store(0, Ordering::Relaxed);
        self.tx_pack_id.store(0, Ordering::Relaxed);
        self.rx_pack_id.store(0, Ordering::Relaxed);
    }
}

pub struct PeerStreamContext {
    is_client: bool,
    client_context: Option<Arc<ClientContext>>,
    peer_stream_key: Arc<PeerStreamKey>,
    once: PeerStreamOnce,
}

pub struct PeerStream {
    to_peer_stream_rx: async_channel::Receiver<PeerStreamRecv>,
    accept_tx: Option<AcceptSenderType>,
    context: Arc<PeerStreamContext>,
}

impl Drop for PeerStream {
    fn drop(&mut self) {
        self.to_peer_stream_rx.close();
    }
}

impl PeerStream {
    pub fn new(
        is_client: bool,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        domain: Option<ArcString>,
        client_context: Option<Arc<ClientContext>>,
        accept_tx: Option<AcceptSenderType>,
        key: String,
    ) -> PeerStream {
        let (to_peer_stream_tx, to_peer_stream_rx) = async_channel::unbounded();
        let peer_stream_key = Arc::new(PeerStreamKey {
            key,
            local_addr,
            remote_addr,
            domain,
            to_peer_stream_tx,
        });

        PeerStream {
            to_peer_stream_rx,
            accept_tx,
            context: Arc::new(PeerStreamContext {
                is_client,
                client_context,
                peer_stream_key,
                once: PeerStreamOnce::new(),
            }),
        }
    }

    pub fn peer_stream_key(&self) -> Arc<PeerStreamKey> {
        self.context.peer_stream_key.clone()
    }

    pub async fn start(
        mut peer_stream: PeerStream,
        stream: StreamFlow,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> Result<()> {
        if peer_stream.context.is_client {
            let async_ = Box::pin(async move {
                peer_stream
                    .do_start(stream)
                    .await
                    .map_err(|e| anyhow!("err:PeerStream::do_start => e:{}", e))
            });
            if cfg!(feature = "anyruntime-tokio-spawn-local") {
                any_base::executor_local_spawn::_start_and_free(
                    move || async move { async_.await },
                );
            } else if cfg!(feature = "anyruntime-tokio-spawn") {
                any_base::executor_spawn::_start_and_free(move || async move { async_.await });
            } else {
                run_time.spawn(Box::pin(async move {
                    let _ = async_.await;
                }));
            }
        } else {
            let ret: Result<()> = async {
                peer_stream
                    .do_start(stream)
                    .await
                    .map_err(|e| anyhow!("err:PeerStream::do_start => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:do_start => e:{}", e));
        }
        Ok(())
    }

    pub async fn stream<R: AsyncRead + std::marker::Unpin, W: AsyncWrite + std::marker::Unpin>(
        &mut self,
        r: &mut R,
        w: &mut W,
        stream_info: ArcMutex<StreamFlowInfo>,
    ) -> Result<bool> {
        let ret: Result<(Option<PeerStreamRecv>, bool)> = async {
            tokio::select! {
                biased;
                msg = self.peer_stream_recv() => {
                    if msg.is_err() {
                        return Ok((None, true))
                    }
                   return Ok((Some(msg?), false));
                }
                ret = self.read_tunnel_hello( r) => {
                    ret?;
                    return Ok((None, false));
                }
                else => {
                    return Err(anyhow!("err:start_cmd"))?;
                }
            }
        }
        .await;
        let (msg, is_close) = ret?;
        if is_close {
            return Ok(true);
        }
        let msg = match msg {
            Some(PeerStreamRecv::PeerStreamRecvHello(msg)) => msg,
            None => return Ok(false),
        };

        let is_close = self.do_stream(r, w, msg, stream_info).await?;
        Ok(is_close)
    }

    pub async fn do_start(&mut self, mut stream: StreamFlow) -> Result<()> {
        let stream_info = ArcMutex::new(StreamFlowInfo::new());
        stream.set_stream_info(Some(stream_info.clone()));
        let (mut r, mut w) = tokio::io::split(stream);
        loop {
            #[cfg(feature = "anydebug")]
            log::info!(
                "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} do_start",
                { self.context.once.session_id.lock().unwrap() },
                get_flag(self.context.is_client),
                self.context.once.peer_stream_id.load(Ordering::Relaxed),
                self.context.once.peer_stream_index.load(Ordering::Relaxed),
            );

            self.context.once.init();
            let ret = self.stream(&mut r, &mut w, stream_info.clone()).await;
            if let Err(e) = ret {
                let err = { stream_info.get().err.clone() };
                if err as i32 >= StreamFlowErr::WriteTimeout as i32 {
                    return Err(anyhow!("err:do_stream => e:{}", e))?;
                }
                let _ = w.flush().await;
                //let _ = w.shutdown().await;
                return Ok(());
            }

            if ret? {
                let _ = w.flush().await;
                //let _ = w.shutdown().await;
                return Ok(());
            }
        }
    }

    pub async fn peer_stream_recv(&self) -> Result<PeerStreamRecv> {
        let msg = self
            .to_peer_stream_rx
            .recv()
            .await
            .map_err(|e| anyhow!("err:to_peer_stream_rx close => err:{}", e))?;
        Ok(msg)
    }

    pub async fn read_tunnel_hello<R: AsyncRead + std::marker::Unpin>(
        &self,
        r: &mut R,
    ) -> Result<()> {
        let mut is_hello = false;
        loop {
            if is_hello {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }

            let tunnel_hello = protopack::read_tunnel_hello(r).await.map_err(|e| {
                anyhow!(
                    "flag:{}, peer_stream_id:{}, peer_stream_index:{} read_tunnel_hello => err:{}",
                    get_flag(self.context.is_client),
                    self.context.once.peer_stream_id.load(Ordering::Relaxed),
                    self.context.once.peer_stream_index.load(Ordering::Relaxed),
                    e
                )
            })?;

            self.accept_tx
                .as_ref()
                .unwrap()
                .send(ServerRecv::ServerRecvHello(ServerRecvHello {
                    peer_stream_key: self.context.peer_stream_key.clone(),
                    tunnel_hello,
                }))
                .await
                .map_err(|e| anyhow!("err:read_tunnel_hello => err:{}", e))?;
            is_hello = true;
        }
    }

    pub async fn do_stream<
        R: AsyncRead + std::marker::Unpin,
        W: AsyncWrite + std::marker::Unpin,
    >(
        &mut self,
        r: &mut R,
        w: &mut W,
        msg: PeerStreamRecvHello,
        stream_info: ArcMutex<StreamFlowInfo>,
    ) -> Result<bool> {
        let PeerStreamRecvHello {
            stream_to_peer_stream_rx,
            peer_stream_to_peer_client_tx,
            tunnel_hello,
            min_stream_cache_size,
            peer_stream_index,
            channel_size,
            peer_stream_tx_pack_id,
            peer_stream_rx_pack_id,
            session_id,
            is_error,
            peer_stream_id,
        } = msg;

        let peer_stream_id = if peer_stream_id.is_some() {
            peer_stream_id.unwrap()
        } else {
            0
        };
        self.context
            .once
            .peer_stream_id
            .store(peer_stream_id, Ordering::Relaxed);
        self.context
            .once
            .peer_stream_index
            .store(peer_stream_index, Ordering::Relaxed);
        {
            *self.context.once.session_id.lock().unwrap() = session_id
        };

        #[cfg(feature = "anydebug")]
        {
            log::info!(
                    "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} do_stream start ",
                {self.context.once.session_id.lock().unwrap()},
                    get_flag(self.context.is_client),
                    self.context.once.peer_stream_id.load(Ordering::Relaxed),
                    self.context.once.peer_stream_index.load(Ordering::Relaxed),
                );
        }

        let ret: Result<()> = async {
            if self.context.is_client && tunnel_hello.is_none() {
                return Err(anyhow!("err:self.is_client && tunnel_hello.is_none()"));
            }

            let peer_stream_to_peer_client_tx = peer_stream_to_peer_client_tx.clone_and_ref();
            if peer_stream_to_peer_client_tx.is_none() {
                return Err(anyhow!("peer_stream_to_peer_client_tx.is_none"));
            }
            let peer_stream_to_peer_client_tx = peer_stream_to_peer_client_tx.unwrap();

            let mut peer_stream_read = PeerStreamRead::new(r,
                                                           peer_stream_to_peer_client_tx,
                                        stream_to_peer_stream_rx.clone(),
                                        min_stream_cache_size,
                                        channel_size,
                                        peer_stream_rx_pack_id,
                                                       self.context.clone(),
            );
            let mut peer_stream_write = PeerStreamWrite::new(
                w,
                stream_to_peer_stream_rx.clone(),
                tunnel_hello,
                min_stream_cache_size,
                peer_stream_tx_pack_id,
                self.context.clone(),
            );

            tokio::select! {
                biased;
                ret = peer_stream_read.start() => {
                    ret.map_err(|e| anyhow!("err:read_stream => flag:{}, e:{}", get_flag(self.context.is_client), e))?;
                    Ok(())
                }
                ret = peer_stream_write.start() => {
                    ret.map_err(|e| anyhow!("err:write_stream => flag:{}, e:{}", get_flag(self.context.is_client), e))?;
                    Ok(())
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        }
        .await;

        let mut _err_num = 0;
        self.close(stream_to_peer_stream_rx);
        if let Err(e) = ret {
            let tx_pack_id = self.context.once.tx_pack_id.load(Ordering::SeqCst);
            let rx_pack_id = self.context.once.rx_pack_id.load(Ordering::SeqCst);
            let err = { stream_info.get().err.clone() };
            if err.clone() as i32 >= StreamFlowErr::WriteTimeout as i32 {
                _err_num = 1;
                peer_stream_to_peer_client_tx.close();
                is_error.store(true, Ordering::Relaxed);
            } else {
                if tx_pack_id > 0 || rx_pack_id > 0 {
                    _err_num = 2;
                    peer_stream_to_peer_client_tx.close();
                    is_error.store(true, Ordering::Relaxed);
                }
            }
            #[cfg(feature = "anyerror")]
            if _err_num > 0 {
                log::error!(
                    "err:do_stream err_num:{} => flag:{}, session_id:{:?}, peer_stream_id:{}, peer_stream_index:{}, \
                        tx_pack_id:{}, rx_pack_id:{}, peer_stream_key.local_addr:{}, \
                        peer_stream_key.remote_addr:{} min_stream_cache_size:{}, \
                        e:{}, err:{:?}",
                    _err_num,
                    get_flag(self.context.is_client),
                    {self.context.once.session_id.lock().unwrap()},
                    peer_stream_id,
                    peer_stream_index,
                    tx_pack_id,
                    rx_pack_id,
                    self.context.peer_stream_key.local_addr,
                    self.context.peer_stream_key.remote_addr,
                    min_stream_cache_size,
                    e,
                    err
                );
            }

            return Err(anyhow!(
                "err:do_stream => flag:{}, session_id:{:?}, min_stream_cache_size:{}, e:{}",
                get_flag(self.context.is_client),
                { self.context.once.session_id.lock().unwrap() },
                min_stream_cache_size,
                e
            ));
        }

        if self.context.client_context.is_some() {
            self.context
                .client_context
                .as_ref()
                .unwrap()
                .add_peer_stream_key(
                    self.context.peer_stream_key.key.clone(),
                    self.context.peer_stream_key.clone(),
                    min_stream_cache_size,
                );
        }

        if min_stream_cache_size <= 0 {
            return Ok(true);
        }
        Ok(false)
    }

    fn close(&self, stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>) {
        log::debug!("stream_to_peer_stream_rx close");
        stream_to_peer_stream_rx.close();
    }
}

struct PeerStreamRead<'a, R: AsyncRead + std::marker::Unpin> {
    r: &'a mut R,
    peer_stream_to_peer_client_tx: PeerStreamToPeerClientTx,
    _stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
    _min_stream_cache_size: usize,
    channel_size: usize,
    peer_stream_rx_pack_id: Arc<AtomicU32>,
    context: Arc<PeerStreamContext>,
}

impl<R: AsyncRead + std::marker::Unpin> PeerStreamRead<'_, R> {
    pub fn new(
        r: &mut R,
        peer_stream_to_peer_client_tx: PeerStreamToPeerClientTx,
        _stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
        _min_stream_cache_size: usize,
        channel_size: usize,
        peer_stream_rx_pack_id: Arc<AtomicU32>,
        context: Arc<PeerStreamContext>,
    ) -> PeerStreamRead<R> {
        PeerStreamRead {
            r,
            peer_stream_to_peer_client_tx,
            _stream_to_peer_stream_rx,
            _min_stream_cache_size,
            channel_size,
            peer_stream_rx_pack_id,
            context,
        }
    }

    async fn read_stream(&mut self) -> Result<()> {
        #[cfg(feature = "anydebug")]
        log::info!(
            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read_stream",
            {self.context.once.session_id.lock().unwrap()},
            get_flag(self.context.is_client),
            self.context.once.peer_stream_id.load(Ordering::Relaxed),
            self.context.once.peer_stream_index.load(Ordering::Relaxed),
        );

        let mut slice = [0u8; protopack::TUNNEL_MAX_HEADER_SIZE];
        let buffer_pool = TunnelDynamicPool {
            #[cfg(feature = "anypool-dynamic-pool")]
            tunnel_data: DynamicPool::new(1, self.channel_size * 2, TunnelData::default),
            #[cfg(not(feature = "anypool-dynamic-pool"))]
            tunnel_data: DynamicPoolTunnelData::new(),
        };

        //err 直接退出   ok 等待退出
        loop {
            let pack = protopack::read_pack_all(&mut self.r, &mut slice, &buffer_pool).await;
            if let Err(e) = pack {
                return Err(anyhow!(
                    "err:read_pack => flag:{}, e:{}",
                    get_flag(self.context.is_client),
                    e
                ))?;
            }
            let pack = pack.unwrap();
            match &pack {
                TunnelPack::TunnelHello(value) => {
                    return Err(anyhow!("err:TunnelHello => TunnelHello:{:?}", value))?;
                }
                TunnelPack::TunnelClose(value) => {
                    #[cfg(feature = "anydebug")]
                    log::info!(
                            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read TunnelClose local value:{}, value:{}",
                        {self.context.once.session_id.lock().unwrap()},
                            get_flag(self.context.is_client),
                            self.context.once.peer_stream_id.load(Ordering::Relaxed),
                            self.context.once.peer_stream_index.load(Ordering::Relaxed),
                            get_flag(self.context.is_client),
                            get_flag(value.is_client),
                        );

                    if value.is_client != self.context.is_client {
                        {
                            #[cfg(feature = "anydebug")]
                            {
                                if self
                                    .context
                                    .once
                                    .tunnel_peer_close
                                    .lock()
                                    .unwrap()
                                    .is_some()
                                {
                                    log::info!(
                                            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read tunnel_peer_close is_some",
                                            {self.context.once.session_id.lock().unwrap()},
                                            get_flag(self.context.is_client),
                                            self.context.once.peer_stream_id.load(Ordering::Relaxed),
                                            self.context.once.peer_stream_index.load(Ordering::Relaxed),
                                        );
                                }
                            }
                            {
                                *self.context.once.tunnel_peer_close.lock().unwrap() =
                                    Some(value.clone())
                            };
                        }

                        if !self.context.once.is_send_close.load(Ordering::SeqCst) {
                            #[cfg(feature = "anydebug")]
                            {
                                log::info!(
                                            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read value.is_client != self.is_client",
                                    { self.context.once.session_id.lock().unwrap()},
                                            get_flag(self.context.is_client),
                                            self.context.once.peer_stream_id.load(Ordering::Relaxed),
                                            self.context.once.peer_stream_index.load(Ordering::Relaxed),
                                        );
                            }
                            return Ok(());
                        }

                        continue;
                    }

                    #[cfg(feature = "anydebug")]
                    log::info!(
                            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read value.is_client == self.is_client",
                        {self.context.once.session_id.lock().unwrap()},
                            get_flag(self.context.is_client),
                            self.context.once.peer_stream_id.load(Ordering::Relaxed),
                            self.context.once.peer_stream_index.load(Ordering::Relaxed),
                        );

                    return Ok(());
                }
                TunnelPack::TunnelData(_value) => {
                    self.context
                        .once
                        .rx_pack_id
                        .store(_value.header.pack_id, Ordering::SeqCst);
                    loop {
                        let curr = self.peer_stream_rx_pack_id.load(Ordering::SeqCst);
                        if _value.header.pack_id <= curr {
                            break;
                        }
                        let ret = self.peer_stream_rx_pack_id.compare_exchange(
                            curr,
                            _value.header.pack_id,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                        if ret.is_ok() {
                            break;
                        }
                    }

                    let _ret = self.peer_stream_to_peer_client_tx.tx.send(pack).await;
                    #[cfg(feature = "anyerror")]
                    if let Err(e) = _ret {
                        log::error!("err:peer_stream_to_peer_client_tx send => e:{}", e);
                    }
                }
                _ => {
                    let _ret = self.peer_stream_to_peer_client_tx.tx.send(pack).await;
                    #[cfg(feature = "anyerror")]
                    if let Err(e) = _ret {
                        log::error!("err:peer_stream_to_peer_client_tx send => e:{}", e);
                    }
                }
            }
        }
    }
    async fn start(&mut self) -> Result<()> {
        self.read_stream().await?;

        #[cfg(feature = "anydebug")]
        log::info!(
            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read break",
            {self.context.once.session_id.lock().unwrap()},
            get_flag(self.context.is_client),
            self.context.once.peer_stream_id.load(Ordering::Relaxed),
            self.context.once.peer_stream_index.load(Ordering::Relaxed),
        );

        self.context.once.close_num.fetch_add(1, Ordering::SeqCst);
        //等待write close， 让write去关闭

        #[cfg(feature = "anydebug")]
        log::info!(
            "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream read wait exit",
            { self.context.once.session_id.lock().unwrap()},
            get_flag(self.context.is_client),
            self.context.once.peer_stream_id.load(Ordering::Relaxed),
            self.context.once.peer_stream_index.load(Ordering::Relaxed),
        );
        //120秒后强制退出
        tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;
        return Err(anyhow!("read close timeout"));
    }
}

struct PeerStreamWrite<'a, W: AsyncWrite + std::marker::Unpin> {
    w: &'a mut W,
    stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
    tunnel_hello: Option<TunnelHello>,
    _min_stream_cache_size: usize,
    peer_stream_tx_pack_id: Arc<AtomicU32>,
    context: Arc<PeerStreamContext>,
}

impl<W: AsyncWrite + std::marker::Unpin> PeerStreamWrite<'_, W> {
    pub fn new(
        w: &mut W,
        stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
        tunnel_hello: Option<TunnelHello>,
        _min_stream_cache_size: usize,
        peer_stream_tx_pack_id: Arc<AtomicU32>,
        context: Arc<PeerStreamContext>,
    ) -> PeerStreamWrite<W> {
        PeerStreamWrite {
            w,
            stream_to_peer_stream_rx,
            tunnel_hello,
            _min_stream_cache_size,
            peer_stream_tx_pack_id,
            context,
        }
    }
    pub async fn write_tunnel_hello(&mut self) -> Result<()> {
        if self.tunnel_hello.is_none() {
            return Ok(());
        }

        #[cfg(feature = "anydebug")]
        {
            log::info!(
                        "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} write_stream TunnelHello ",
                {self.context.once.session_id.lock().unwrap()},
                        get_flag(self.context.is_client),
                        self.context.once.peer_stream_id.load(Ordering::Relaxed),
                        self.context.once.peer_stream_index.load(Ordering::Relaxed),
                    );
        }
        let tunnel_hello = self.tunnel_hello.take().unwrap();
        log::trace!("write_stream tunnel_hello:{:?}", tunnel_hello);
        {
            *self.context.once.session_id.lock().unwrap() = Some(tunnel_hello.session_id.clone());
        }
        protopack::write_pack(
            &mut self.w,
            TunnelHeaderType::TunnelHello,
            &tunnel_hello,
            true,
        )
        .await
        .map_err(|e| anyhow!("e:{}", e))?;
        Ok(())
    }

    pub async fn write_pack(&mut self) -> Result<Option<()>> {
        loop {
            let is_tunnel_peer_close = {
                self.context
                    .once
                    .tunnel_peer_close
                    .lock()
                    .unwrap()
                    .is_some()
            };
            if is_tunnel_peer_close {
                #[cfg(feature = "anydebug")]
                log::info!(
                        "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream tunnel_peer_close",
                    { self.context.once.session_id.lock().unwrap()},
                        get_flag(self.context.is_client),
                        self.context.once.peer_stream_id.load(Ordering::Relaxed),
                        self.context.once.peer_stream_index.load(Ordering::Relaxed),
                    );

                return Ok(None);
            }

            let pack = tokio::time::timeout(
                tokio::time::Duration::from_secs(1),
                self.stream_to_peer_stream_rx.recv(),
            )
            .await;

            if let Err(_) = pack {
                #[cfg(feature = "anydebug")]
                {
                    log::trace!(
                                "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} write_stream timeout ",
                        {self.context.once.session_id.lock().unwrap()},
                                get_flag(self.context.is_client),
                                self.context.once.peer_stream_id.load(Ordering::Relaxed),
                                self.context.once.peer_stream_index.load(Ordering::Relaxed),
                            );
                }
                continue;
            }

            let pack = pack.unwrap();
            if let Err(_) = pack {
                if self
                    .context
                    .once
                    .tunnel_peer_close
                    .lock()
                    .unwrap()
                    .is_some()
                {
                    return Ok(Some(()));
                }
                #[cfg(feature = "anydebug")]
                log::info!(
                        "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream write TunnelClose",
                    {self.context.once.session_id.lock().unwrap()},
                        get_flag(self.context.is_client),
                        self.context.once.peer_stream_id.load(Ordering::Relaxed),
                        self.context.once.peer_stream_index.load(Ordering::Relaxed),
                    );

                self.context
                    .once
                    .is_send_close
                    .store(true, Ordering::SeqCst);
                protopack::write_pack(
                    &mut self.w,
                    TunnelHeaderType::TunnelClose,
                    &TunnelClose {
                        is_client: self.context.is_client,
                    },
                    true,
                )
                .await
                .map_err(|e| anyhow!("e:{}", e))?;
                return Ok(None);
            }

            let pack = pack.unwrap();
            match &pack {
                TunnelPack::TunnelHello(value) => {
                    log::trace!("write_stream TunnelHello:{:?}", value);
                    return Err(anyhow!("err: write_stream TunnelHello:{:?}", value));
                }
                TunnelPack::TunnelClose(value) => {
                    log::trace!("write_stream TunnelClose:{:?}", value);
                    return Err(anyhow!("err: write_stream TunnelClose:{:?}", value));
                }
                TunnelPack::TunnelData(value) => {
                    log::trace!(
                        "write_stream TunnelData:{:?}, data len:{}",
                        value.header,
                        value.datas.len()
                    );

                    self.context
                        .once
                        .tx_pack_id
                        .store(value.header.pack_id, Ordering::SeqCst);
                    loop {
                        let curr = self.peer_stream_tx_pack_id.load(Ordering::SeqCst);
                        if value.header.pack_id <= curr {
                            break;
                        }
                        let ret = self.peer_stream_tx_pack_id.compare_exchange(
                            curr,
                            value.header.pack_id,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                        if ret.is_ok() {
                            break;
                        }
                    }
                    protopack::write_tunnel_data(&mut self.w, value)
                        .await
                        .map_err(|e| anyhow!("e:{}", e))?;
                }
                TunnelPack::TunnelAddConnect(value) => {
                    log::trace!("write_stream TunnelAddConnect:{:?}", value);
                    protopack::write_pack(
                        &mut self.w,
                        TunnelHeaderType::TunnelAddConnect,
                        value,
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("e:{}", e))?;
                }
                TunnelPack::TunnelMaxConnect(value) => {
                    log::trace!("write_stream TunnelMaxConnect:{:?}", value);
                    protopack::write_pack(
                        &mut self.w,
                        TunnelHeaderType::TunnelMaxConnect,
                        value,
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("e:{}", e))?;
                }
            };
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        self.write_tunnel_hello().await?;

        #[cfg(feature = "anydebug")]
        {
            log::info!(
                "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} write_stream ",
                { self.context.once.session_id.lock().unwrap() },
                get_flag(self.context.is_client),
                self.context.once.peer_stream_id.load(Ordering::Relaxed),
                self.context.once.peer_stream_index.load(Ordering::Relaxed),
            );
        }

        self.write_pack().await?;

        #[cfg(feature = "anydebug")]
        log::info!(
                "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream write start exit",
                {self.context.once.session_id.lock().unwrap()},
                get_flag(self.context.is_client),
                self.context.once.peer_stream_id.load(Ordering::Relaxed),
                self.context.once.peer_stream_index.load(Ordering::Relaxed),
            );

        self.context.once.close_num.fetch_add(1, Ordering::SeqCst);
        let mut wait_num = 0;
        loop {
            let tunnel_peer_close = { self.context.once.tunnel_peer_close.lock().unwrap().take() };
            if tunnel_peer_close.is_some() {
                #[cfg(feature = "anydebug")]
                log::info!(
                        "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream write TunnelClose take from read",
                    {self.context.once.session_id.lock().unwrap()},
                        get_flag(self.context.is_client),
                        self.context.once.peer_stream_id.load(Ordering::Relaxed),
                        self.context.once.peer_stream_index.load(Ordering::Relaxed),
                    );
                let tunnel_peer_close = tunnel_peer_close.unwrap();
                protopack::write_pack(
                    &mut self.w,
                    TunnelHeaderType::TunnelClose,
                    &tunnel_peer_close,
                    true,
                )
                .await
                .map_err(|e| anyhow!("e:{}", e))?;
            }

            if self.context.once.close_num.load(Ordering::SeqCst) >= 2 {
                break;
            }

            #[cfg(feature = "anydebug")]
            log::info!(
                    "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream write sleep",
                {self.context.once.session_id.lock().unwrap()},
                    get_flag(self.context.is_client),
                    self.context.once.peer_stream_id.load(Ordering::Relaxed),
                    self.context.once.peer_stream_index.load(Ordering::Relaxed),
                );
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            wait_num += 1;
            if wait_num > 400 {
                return Err(anyhow!("err:peer_stream write wait stop"));
            }
        }

        #[cfg(feature = "anydebug")]
        log::info!(
                "session_id:{:?}, flag:{}, peer_stream_id:{}, peer_stream_index:{} peer_stream write end exit",
                {self.context.once.session_id.lock().unwrap()},
                get_flag(self.context.is_client),
                self.context.once.peer_stream_id.load(Ordering::Relaxed),
                self.context.once.peer_stream_index.load(Ordering::Relaxed),
            );

        Ok(())
    }
}
