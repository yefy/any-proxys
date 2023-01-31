use super::protopack;
#[cfg(not(feature = "anytunnel-dynamic-pool"))]
use super::protopack::DynamicPoolTunnelData;
#[cfg(feature = "anytunnel-dynamic-pool")]
use super::protopack::TunnelData;
use super::protopack::TunnelDynamicPool;
use super::protopack::TunnelHeaderType;
use super::protopack::TunnelPack;
use super::server::ServerRecv;
use super::stream_flow::StreamFlow;
use super::stream_flow::StreamFlowErr;
use super::stream_flow::StreamFlowInfo;
#[cfg(feature = "anytunnel-dynamic-pool")]
use crate::client::AsyncClient;
use crate::protopack::{TunnelClose, TunnelHello};
use crate::server::{AcceptSenderType, ServerRecvHello};
use anyhow::anyhow;
use anyhow::Result;
#[cfg(feature = "anytunnel-dynamic-pool")]
use dynamic_pool::DynamicPool;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::get_flag;
#[cfg(feature = "anytunnel-debug")]
use crate::DEFAULT_PRINT_NUM;

pub struct PeerStreamKey {
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub to_peer_stream_tx: async_channel::Sender<PeerStreamRecv>,
}

pub struct PeerStreamRecvHello {
    pub stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
    pub peer_stream_to_peer_client_tx: async_channel::Sender<TunnelPack>,
    pub tunnel_hello: Option<TunnelHello>,
    pub min_stream_cache_size: usize,
    pub peer_stream_index: usize,
    pub channel_size: usize,
}

pub enum PeerStreamRecv {
    PeerStreamRecvHello(PeerStreamRecvHello),
}

pub struct PeerStream {
    is_client: bool,
    peer_stream_key: Arc<PeerStreamKey>,
    to_peer_stream_rx: async_channel::Receiver<PeerStreamRecv>,
    async_client: Option<Arc<AsyncClient>>,
    close_num: Arc<AtomicUsize>,
    accept_tx: Option<AcceptSenderType>,
    tunnel_peer_close: Mutex<Option<TunnelClose>>,
    is_send_close: AtomicBool,
    session_id: Mutex<Option<String>>,
    peer_stream_index: AtomicUsize,
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
        async_client: Option<Arc<AsyncClient>>,
        accept_tx: Option<AcceptSenderType>,
    ) -> PeerStream {
        let (to_peer_stream_tx, to_peer_stream_rx) = async_channel::unbounded();
        let peer_stream_key = Arc::new(PeerStreamKey {
            local_addr,
            remote_addr,
            to_peer_stream_tx,
        });
        PeerStream {
            is_client,
            peer_stream_key,
            to_peer_stream_rx,
            async_client,
            close_num: Arc::new(AtomicUsize::new(0)),
            accept_tx,
            tunnel_peer_close: Mutex::new(None),
            is_send_close: AtomicBool::new(false),
            session_id: Mutex::new(None),
            peer_stream_index: AtomicUsize::new(0),
        }
    }

    pub fn init(&mut self) {
        self.close_num.store(0, Ordering::SeqCst);
        self.tunnel_peer_close.lock().unwrap().take();
        self.is_send_close.store(false, Ordering::SeqCst);
        *self.session_id.lock().unwrap() = None;
    }

    pub fn peer_stream_key(&self) -> Arc<PeerStreamKey> {
        self.peer_stream_key.clone()
    }

    pub async fn start(mut peer_stream: PeerStream, stream: StreamFlow) -> Result<()> {
        if peer_stream.is_client {
            any_base::executor_spawn::_start_and_free(move || async move {
                peer_stream
                    .do_start(stream)
                    .await
                    .map_err(|e| anyhow!("err:PeerStream::do_start => e:{}", e))
            });
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

    pub async fn do_start(&mut self, mut stream: StreamFlow) -> Result<()> {
        let stream_info = Arc::new(std::sync::Mutex::new(StreamFlowInfo::new()));
        stream.set_stream_info(Some(stream_info.clone()));

        let (r, w) = tokio::io::split(stream);
        let mut r = any_base::io::buf_reader::BufReader::new(r);
        let mut w = any_base::io::buf_writer::BufWriter::new(w);

        loop {
            let ret: Result<()> = async {
                #[cfg(feature = "anytunnel-debug")]
                log::info!(
                    "flag:{}, peer_stream_index:{} do_start",
                    get_flag(self.is_client),
                    self.peer_stream_index.load(Ordering::Relaxed),
                );
                self.init();
                let ret: Result<Option<PeerStreamRecv>> = async {
                    tokio::select! {
                        biased;
                        recv_msg = self.peer_stream_recv() => {
                           return Ok(Some(recv_msg?));
                        }
                        ret = self.read_tunnel_hello(&mut r) => {
                            ret?;
                            return Ok(None);
                        }
                        else => {
                            return Err(anyhow!("err:start_cmd"))?;
                        }
                    }
                }
                .await;
                let recv_msg = ret?;
                let recv_msg = match recv_msg {
                    Some(PeerStreamRecv::PeerStreamRecvHello(recv_msg)) => recv_msg,
                    None => return Ok(()),
                };

                self.peer_stream_index
                    .store(recv_msg.peer_stream_index, Ordering::Relaxed);
                #[cfg(feature = "anytunnel-debug")]
                let min_stream_cache_size = recv_msg.min_stream_cache_size;
                #[cfg(feature = "anytunnel-debug")]
                let peer_stream_index = recv_msg.peer_stream_index;
                #[cfg(feature = "anytunnel-debug")]
                {
                    log::info!(
                        "flag:{}, peer_stream_index:{}, do_stream start min_stream_cache_size:{}",
                        get_flag(self.is_client),
                        peer_stream_index,
                        min_stream_cache_size,
                    );
                }
                self.do_stream(&mut r, &mut w, recv_msg).await?;
                #[cfg(feature = "anytunnel-debug")]
                {
                    log::info!(
                        "flag:{}, peer_stream_index:{}, do_stream wait min_stream_cache_size:{}",
                        get_flag(self.is_client),
                        peer_stream_index,
                        min_stream_cache_size,
                    );
                }
                Ok(())
            }
            .await;
            if let Err(e) = ret {
                #[cfg(feature = "anytunnel-debug")]
                {
                    log::info!(
                        "flag:{}, peer_stream_index:{}, do_stream err min_stream_cache_size:{}",
                        get_flag(self.is_client),
                        peer_stream_index,
                        min_stream_cache_size,
                    );
                }

                let err = { stream_info.lock().unwrap().err.clone() };
                if err as i32 >= StreamFlowErr::WriteTimeout as i32 {
                    return Err(anyhow!("err:do_stream => e:{}", e))?;
                }
                return Ok(());
            }
        }
    }
    pub async fn peer_stream_recv(&self) -> Result<PeerStreamRecv> {
        let recv_msg = self
            .to_peer_stream_rx
            .recv()
            .await
            .map_err(|e| anyhow!("err:to_peer_stream_rx close => err:{}", e))?;
        Ok(recv_msg)
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

            #[cfg(feature = "anytunnel-debug")]
            log::info!(
                "flag:{}, peer_stream_index:{} read_tunnel_hello start ",
                get_flag(self.is_client),
                self.peer_stream_index.load(Ordering::Relaxed),
            );
            let tunnel_hello = protopack::read_tunnel_hello(r).await.map_err(|e| {
                anyhow!(
                    "flag:{}, peer_stream_index:{} read_tunnel_hello => err:{}",
                    get_flag(self.is_client),
                    self.peer_stream_index.load(Ordering::Relaxed),
                    e
                )
            })?;

            {
                *self.session_id.lock().unwrap() = Some(tunnel_hello.session_id.clone());
            }
            self.accept_tx
                .as_ref()
                .unwrap()
                .send(ServerRecv::ServerRecvHello(ServerRecvHello {
                    peer_stream_key: self.peer_stream_key.clone(),
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
        recv_msg: PeerStreamRecvHello,
    ) -> Result<()> {
        let PeerStreamRecvHello {
            stream_to_peer_stream_rx,
            peer_stream_to_peer_client_tx,
            tunnel_hello,
            min_stream_cache_size,
            peer_stream_index: _,
            channel_size,
        } = recv_msg;

        let ret: Result<()> = async {
            if self.is_client && tunnel_hello.is_none() {
                return Err(anyhow!("err:self.is_client && tunnel_hello.is_none()"));
            }

            tokio::select! {
                biased;
                ret = self.read_stream(
                    r,
                    peer_stream_to_peer_client_tx.clone(),
                    stream_to_peer_stream_rx.clone(),
                    min_stream_cache_size,
                    channel_size,
                ) => {
                    ret.map_err(|e| anyhow!("err:read_stream => e:{}", e))?;
                    Ok(())
                }
                ret = self.write_stream(
                    w,
                    peer_stream_to_peer_client_tx.clone(),
                    stream_to_peer_stream_rx.clone(),
                    tunnel_hello,
                    min_stream_cache_size
                ) => {
                    ret.map_err(|e| anyhow!("err:write_stream => e:{}", e))?;
                    Ok(())
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        }
        .await;
        if let Err(e) = ret {
            self.close(peer_stream_to_peer_client_tx, stream_to_peer_stream_rx);
            return Err(anyhow!("err:do_stream => e:{}", e));
        }
        if min_stream_cache_size <= 0 {
            return Err(anyhow!("err:close"));
        }
        Ok(())
    }

    async fn read_stream<R: AsyncRead + std::marker::Unpin>(
        &self,
        mut r: R,
        peer_stream_to_peer_client_tx: async_channel::Sender<TunnelPack>,
        _stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
        min_stream_cache_size: usize,
        channel_size: usize,
    ) -> Result<()> {
        let mut slice = [0u8; protopack::TUNNEL_MAX_HEADER_SIZE];
        let buffer_pool = TunnelDynamicPool {
            #[cfg(feature = "anytunnel-dynamic-pool")]
            tunnel_data: DynamicPool::new(8, channel_size * 2, TunnelData::default),
            #[cfg(not(feature = "anytunnel-dynamic-pool"))]
            tunnel_data: DynamicPoolTunnelData::new(),
        };

        #[cfg(feature = "anytunnel-debug")]
        {
            log::info!(
                "flag:{}, peer_stream_index:{} read_stream",
                get_flag(self.is_client),
                self.peer_stream_index.load(Ordering::Relaxed),
            );
        }

        //err 直接退出   ok 等待退出
        let ret: Result<()> = async {
            loop {
                //write已經等待退出了
                if self.close_num.load(Ordering::SeqCst) >= 1 {
                    if min_stream_cache_size <= 0 {
                        return Ok(());
                    }
                }
                let pack =  protopack::read_pack_all(&mut r, &mut slice, &buffer_pool).await;
                if let Err(e) = pack {
                    return Err(anyhow!("err:read_pack => e:{}", e))?;
                }
                let pack = pack.unwrap();
                match &pack {
                    TunnelPack::TunnelHello(value) => {
                        return Err(anyhow!("err:TunnelHello => TunnelHello:{:?}", value))?;
                    }
                    TunnelPack::TunnelClose(value) => {
                        #[cfg(feature = "anytunnel-debug")]
                        log::info!(
                            "flag:{}, peer_stream_index:{} peer_stream read TunnelClose local value:{}, value:{}",
                            get_flag(self.is_client),
                            self.peer_stream_index.load(Ordering::Relaxed),
                            get_flag(self.is_client),
                            get_flag(value.is_client),
                        );

                        if min_stream_cache_size <= 0 {
                            return Err(anyhow!(
                                "err:flag:{} min_stream_cache_size <= 0",
                                get_flag(self.is_client)
                            ));
                        }

                        if value.is_client != self.is_client {
                            {*self.tunnel_peer_close.lock().unwrap() = Some(value.clone());}

                            if !self.is_send_close.load(Ordering::SeqCst) {
                                #[cfg(feature = "anytunnel-debug")]
                                {
                                    log::info!(
                                        "flag:{}, peer_stream_index:{} peer_stream read value.is_client != self.is_client",
                                        get_flag(self.is_client),
                                        self.peer_stream_index.load(Ordering::Relaxed),
                                    );
                                }
                                return Ok(());
                            }

                            continue;
                        }

                        #[cfg(feature = "anytunnel-debug")]
                        log::info!(
                            "flag:{}, peer_stream_index:{} peer_stream read value.is_client == self.is_client",
                            get_flag(self.is_client),
                            self.peer_stream_index.load(Ordering::Relaxed),
                        );

                        return Ok(());
                    }
                    TunnelPack::TunnelData(_value) => {
                        #[cfg(feature = "anytunnel-debug")]
                        {
                            if _value.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                                log::info!(
                                    "flag:{}, peer_stream_index:{} peer_stream read pack_id:{}",
                                    get_flag(self.is_client),
                                    self.peer_stream_index.load(Ordering::Relaxed),
                                    _value.header.pack_id
                                );
                            }
                        }
                        let _ = peer_stream_to_peer_client_tx.send(pack).await;
                    }
                    _ => {
                        let _ = peer_stream_to_peer_client_tx.send(pack).await;
                    }
                }
            }
        }
        .await;
        ret?;

        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream read break",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );

        self.close_num.fetch_add(1, Ordering::SeqCst);
        //等待write close， 让write去关闭

        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream read wait exit",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );
        //15秒后强制退出
        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        Ok(())
    }

    async fn write_stream<W: AsyncWrite + std::marker::Unpin>(
        &self,
        mut w: W,
        peer_stream_to_peer_client_tx: async_channel::Sender<TunnelPack>,
        stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
        tunnel_hello: Option<TunnelHello>,
        min_stream_cache_size: usize,
    ) -> Result<()> {
        if tunnel_hello.is_some() {
            #[cfg(feature = "anytunnel-debug")]
            {
                log::info!(
                    "flag:{}, peer_stream_index:{} write_stream TunnelHello ",
                    get_flag(self.is_client),
                    self.peer_stream_index.load(Ordering::Relaxed),
                );
            }
            let tunnel_hello = tunnel_hello.unwrap();
            log::trace!("write_stream tunnel_hello:{:?}", tunnel_hello);
            {
                *self.session_id.lock().unwrap() = Some(tunnel_hello.session_id.clone());
            }
            protopack::write_pack(&mut w, TunnelHeaderType::TunnelHello, &tunnel_hello, true)
                .await
                .map_err(|e| anyhow!("e:{}", e))?;
        }

        #[cfg(feature = "anytunnel-debug")]
        {
            log::info!(
                "flag:{}, peer_stream_index:{} write_stream ",
                get_flag(self.is_client),
                self.peer_stream_index.load(Ordering::Relaxed),
            );
        }

        let ret: Result<Option<()>> = async {
            loop {
                let is_tunnel_peer_close = { self.tunnel_peer_close.lock().unwrap().is_some() };
                if min_stream_cache_size > 0 && is_tunnel_peer_close {
                    #[cfg(feature = "anytunnel-debug")]
                    log::info!(
                        "flag:{}, peer_stream_index:{} peer_stream tunnel_peer_close",
                        get_flag(self.is_client),
                        self.peer_stream_index.load(Ordering::Relaxed),
                    );

                    return Ok(None);
                }

                let pack = tokio::time::timeout(
                    tokio::time::Duration::from_secs(1),
                    stream_to_peer_stream_rx.recv(),
                )
                .await;

                if let Err(_) = pack {
                    #[cfg(feature = "anytunnel-debug")]
                    {
                        log::trace!(
                            "flag:{}, peer_stream_index:{} write_stream timeout ",
                            get_flag(self.is_client),
                            self.peer_stream_index.load(Ordering::Relaxed),
                        );
                    }
                    continue;
                }

                let pack = pack.unwrap();
                if let Err(_) = pack {
                    if min_stream_cache_size > 0 {
                        if self.tunnel_peer_close.lock().unwrap().is_some() {
                            return Ok(Some(()));
                        }
                        #[cfg(feature = "anytunnel-debug")]
                        log::info!(
                            "flag:{}, peer_stream_index:{} peer_stream write TunnelClose",
                            get_flag(self.is_client),
                            self.peer_stream_index.load(Ordering::Relaxed),
                        );

                        self.is_send_close.store(true, Ordering::SeqCst);
                        protopack::write_pack(
                            &mut w,
                            TunnelHeaderType::TunnelClose,
                            &TunnelClose {
                                is_client: self.is_client,
                            },
                            true,
                        )
                        .await
                        .map_err(|e| anyhow!("e:{}", e))?;
                        return Ok(None);
                    }

                    return Ok(Some(()));
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
                        #[cfg(feature = "anytunnel-debug")]
                        {
                            if value.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                                log::info!(
                                    "flag:{}, peer_stream_index:{} peer_stream write pack_id:{}",
                                    get_flag(self.is_client),
                                    self.peer_stream_index.load(Ordering::Relaxed),
                                    value.header.pack_id
                                );
                            }
                        }
                        protopack::write_tunnel_data(&mut w, value)
                            .await
                            .map_err(|e| anyhow!("e:{}", e))?;
                    }
                    TunnelPack::TunnelAddConnect(value) => {
                        log::trace!("write_stream TunnelAddConnect:{:?}", value);
                        protopack::write_pack(
                            &mut w,
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
                            &mut w,
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
        .await;
        let _ = ret?;

        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream write break",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );

        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream write start exit",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );

        self.close_num.fetch_add(1, Ordering::SeqCst);
        loop {
            if min_stream_cache_size > 0 {
                let tunnel_peer_close = { self.tunnel_peer_close.lock().unwrap().take() };
                if tunnel_peer_close.is_some() {
                    #[cfg(feature = "anytunnel-debug")]
                    log::info!(
                        "flag:{}, peer_stream_index:{} peer_stream write TunnelClose take from read",
                        get_flag(self.is_client),
                        self.peer_stream_index.load(Ordering::Relaxed),
                    );
                    let tunnel_peer_close = tunnel_peer_close.unwrap();
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelClose,
                        &tunnel_peer_close,
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("e:{}", e))?;
                }
            }

            if self.close_num.load(Ordering::SeqCst) >= 2 {
                break;
            }

            #[cfg(feature = "anytunnel-debug")]
            log::info!(
                "flag:{}, peer_stream_index:{} peer_stream write sleep",
                get_flag(self.is_client),
                self.peer_stream_index.load(Ordering::Relaxed),
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
        self.close(peer_stream_to_peer_client_tx, stream_to_peer_stream_rx);

        if min_stream_cache_size > 0 && self.async_client.is_some() {
            self.async_client.as_ref().unwrap().add_peer_stream_key(
                self.peer_stream_key.remote_addr.to_string(),
                self.peer_stream_key.clone(),
                min_stream_cache_size,
            );
        }

        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream write end exit",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );

        Ok(())
    }

    fn close(
        &self,
        peer_stream_to_peer_client_tx: async_channel::Sender<TunnelPack>,
        stream_to_peer_stream_rx: async_channel::Receiver<TunnelPack>,
    ) {
        #[cfg(feature = "anytunnel-debug")]
        log::info!(
            "flag:{}, peer_stream_index:{} peer_stream close",
            get_flag(self.is_client),
            self.peer_stream_index.load(Ordering::Relaxed),
        );

        log::debug!("stream_to_peer_stream_rx close");
        stream_to_peer_stream_rx.close();
        log::debug!("peer_stream_to_peer_client_tx close");
        peer_stream_to_peer_client_tx.close();
    }
}
