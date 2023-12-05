use super::anychannel::AnyAsyncChannelMap;
use super::anychannel::AnyAsyncSenderErr;
use super::peer_stream::PeerStream;
use super::peer_stream_connect::PeerStreamConnect;
use super::protopack;
use super::protopack::TunnelArcPack;
use super::protopack::TunnelClose;
use super::protopack::TunnelHeaderType;
use super::protopack::TunnelHello;
use super::protopack::TUNNEL_VERSION;
use super::server::AcceptSenderType;
use super::stream::Stream;
use super::stream_flow::StreamFlow;
use super::stream_pack::StreamPack;
use super::DEFAULT_STREAM_CHANNEL_SIZE;
use crate::anychannel::AnyAsyncChannel;
use crate::anychannel::AnyAsyncReceiver;
use crate::protopack::{TunnelAddConnect, TunnelMaxConnect};
#[cfg(feature = "anydebug")]
use crate::DEFAULT_PRINT_NUM;
use crate::{
    PeerClientToStreamPackChannel, PeerClientToStreamPackReceiver, PeerStreamToPeerClientChannel,
    PeerStreamToPeerClientReceiver, PeerStreamToPeerClientSender, PeerStreamTunnelData,
    StreamPackToStreamChannel,
};
use anyhow::anyhow;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct PeerClientSender {
    pub peer_client_cmd_tx: mpsc::UnboundedSender<PeerClientCmd>,
    pub stream_pack_to_peer_stream_tx: async_channel::Sender<TunnelArcPack>,
    pub stream_pack_to_peer_stream_rx: async_channel::Receiver<TunnelArcPack>,
    pub peer_stream_to_peer_client_tx: PeerStreamToPeerClientSender,
    pub peer_stream_len: Arc<AtomicI32>,
}

impl PeerClientSender {
    pub fn new(
        peer_client_cmd_tx: mpsc::UnboundedSender<PeerClientCmd>,
        stream_pack_to_peer_stream_tx: async_channel::Sender<TunnelArcPack>,
        stream_pack_to_peer_stream_rx: async_channel::Receiver<TunnelArcPack>,
        peer_stream_to_peer_client_tx: PeerStreamToPeerClientSender,
        peer_stream_len: Arc<AtomicI32>,
    ) -> PeerClientSender {
        PeerClientSender {
            peer_client_cmd_tx,
            stream_pack_to_peer_stream_tx,
            stream_pack_to_peer_stream_rx,
            peer_stream_to_peer_client_tx,
            peer_stream_len,
        }
    }
}

pub enum PeerClientCmd {
    RegisterStream(
        (
            u32,
            mpsc::UnboundedSender<(PeerClientToStreamPackReceiver, SocketAddr, SocketAddr)>,
        ),
    ),
}

pub struct PeerClientAsync {
    session_id: String,
    peer_stream_connect: Option<(Arc<Box<dyn PeerStreamConnect>>, SocketAddr)>,
    peer_stream_max_len: Arc<AtomicUsize>,
    peer_client_sender: PeerClientSender,
    is_max_peer_stream: AtomicBool,
}
impl PeerClientAsync {
    pub fn new(
        session_id: String,
        peer_stream_connect: Option<(Arc<Box<dyn PeerStreamConnect>>, SocketAddr)>,
        peer_stream_max_len: Arc<AtomicUsize>,
        peer_client_sender: PeerClientSender,
    ) -> PeerClientAsync {
        PeerClientAsync {
            session_id,
            peer_stream_connect,
            peer_stream_max_len,
            peer_client_sender,
            is_max_peer_stream: AtomicBool::new(false),
        }
    }
    pub async fn cmd_check_connect(&self) -> Result<()> {
        let mut w_num = 0;
        let max_num = 2;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            //有堵塞了才需要创建链接
            let is_w = {
                let is_full = self
                    .peer_client_sender
                    .stream_pack_to_peer_stream_tx
                    .is_full();
                if is_full {
                    w_num += 1;
                    if w_num <= max_num {
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
                continue;
            }

            let peer_stream_len = self
                .peer_client_sender
                .peer_stream_len
                .load(Ordering::Relaxed);
            let peer_stream_max_len = self.peer_stream_max_len.load(Ordering::Relaxed);

            log::debug!(
                "peer_stream_len:{}, peer_stream_max_len:{}",
                peer_stream_len,
                peer_stream_max_len,
            );

            if self.peer_stream_connect.is_none() {
                if self.is_max_peer_stream.load(Ordering::Relaxed) {
                    continue;
                }

                let peer_client_sender = &self.peer_client_sender;
                let add_connect = Arc::new(TunnelAddConnect {
                    peer_stream_size: 0,
                });
                peer_client_sender
                    .stream_pack_to_peer_stream_tx
                    .send(TunnelArcPack::TunnelAddConnect(add_connect))
                    .await?;
            } else {
                if peer_stream_len < peer_stream_max_len as i32 {
                    let _ = self.add_peer_stream().await;
                }
            }
        }
    }

    pub async fn add_peer_stream(&self) -> Result<()> {
        let (peer_stream_connect, connect_addr) = self.peer_stream_connect.as_ref().unwrap();

        log::debug!("connect_addr:{}", connect_addr);
        let (stream, _, _) = peer_stream_connect
            .connect()
            .await
            .map_err(|e| anyhow!("err:peer_stream_connect.connect => e:{}", e))?;
        let session_id = self.session_id.clone();
        PeerClient::client_insert_peer_stream(true, &self.peer_client_sender, session_id, stream)
            .await
            .map_err(|e| anyhow!("err:client_insert_peer_stream => e:{}", e))?;
        Ok(())
    }
}

pub struct PeerClient {
    arc_async: Arc<PeerClientAsync>,
    peer_stream_max_len: Arc<AtomicUsize>,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    accept_tx: Option<AcceptSenderType>,
    peer_stream_connect: Option<(Arc<Box<dyn PeerStreamConnect>>, SocketAddr)>,
    stream_pack_tx_map: AnyAsyncChannelMap<u32, PeerClientToStreamPackChannel>,
    peer_client_cmd_rx: mpsc::UnboundedReceiver<PeerClientCmd>,
    peer_stream_to_peer_client_rx: PeerStreamToPeerClientReceiver,
    peer_client_sender: PeerClientSender,
}

impl PeerClient {
    pub fn new(
        session_id: String,
        peer_stream_max_len: Arc<AtomicUsize>,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        accept_tx: Option<AcceptSenderType>,
        peer_stream_connect: Option<(Arc<Box<dyn PeerStreamConnect>>, SocketAddr)>,
    ) -> PeerClient {
        let (peer_client_cmd_tx, peer_client_cmd_rx) = mpsc::unbounded_channel();
        let (peer_stream_to_peer_client_tx, peer_stream_to_peer_client_rx) =
            PeerStreamToPeerClientChannel::channel(DEFAULT_STREAM_CHANNEL_SIZE);
        let (stream_pack_to_peer_stream_tx, stream_pack_to_peer_stream_rx) =
            async_channel::bounded(DEFAULT_STREAM_CHANNEL_SIZE * 10);

        let peer_stream_len = Arc::new(AtomicI32::new(0));
        let peer_client_sender = PeerClientSender::new(
            peer_client_cmd_tx,
            stream_pack_to_peer_stream_tx,
            stream_pack_to_peer_stream_rx,
            peer_stream_to_peer_client_tx,
            peer_stream_len,
        );

        PeerClient {
            arc_async: Arc::new(PeerClientAsync::new(
                session_id,
                peer_stream_connect.clone(),
                peer_stream_max_len.clone(),
                peer_client_sender.clone(),
            )),
            peer_stream_max_len,
            local_addr,
            remote_addr,
            accept_tx,
            peer_stream_connect,
            stream_pack_tx_map: AnyAsyncChannelMap::<u32, PeerClientToStreamPackChannel>::new(),
            peer_client_sender,
            peer_client_cmd_rx,
            peer_stream_to_peer_client_rx,
        }
    }

    pub fn get_peer_client_sender(&self) -> PeerClientSender {
        self.peer_client_sender.clone()
    }

    pub async fn start_cmd(&mut self) -> Result<()> {
        let arc_async = self.arc_async.clone();
        let ret: Result<()> = async {
            tokio::select! {
                biased;
                ret =  self.channel_recv() => {
                    ret?;
                    return Ok(());
                }
                ret = arc_async.cmd_check_connect() => {
                    ret?;
                    return Ok(());
                }
                else => {
                    return Err(anyhow!("err:start_cmd"))?;
                }
            }
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:start_cmd => e:{}", e));
        Ok(())
    }

    pub async fn channel_recv(&mut self) -> Result<()> {
        pub enum PeerClientMsgCmd {
            Cmd(Option<PeerClientCmd>),
            PeerStreamToPeerClient(any_base::anychannel::RecvError<PeerStreamTunnelData>),
        }

        loop {
            let msg: Result<PeerClientMsgCmd> = async {
                tokio::select! {
                    biased;
                    msg =  self.peer_client_cmd_rx.recv() => {
                        return Ok(PeerClientMsgCmd::Cmd(msg))
                    }
                    msg = self.peer_stream_to_peer_client_rx.recv() => {
                       return Ok(PeerClientMsgCmd::PeerStreamToPeerClient(msg))
                    }
                    else => {
                        return Err(anyhow!("err:start_cmd"))?;
                    }
                }
            }
            .await;
            let msg = msg?;
            match msg {
                PeerClientMsgCmd::Cmd(msg) => {
                    self.cmd_cmd(msg).await?;
                }
                PeerClientMsgCmd::PeerStreamToPeerClient(msg) => {
                    self.cmd_peer_stream_to_peer_client(msg).await?;
                }
            }
        }
    }

    pub async fn cmd_cmd(&mut self, msg: Option<PeerClientCmd>) -> Result<()> {
        match msg {
            Some(cmd) => match cmd {
                PeerClientCmd::RegisterStream(cmd) => {
                    self.cmd_register_stream(cmd).await?;
                    Ok(())
                }
            },
            None => Ok(()),
        }
    }

    pub async fn cmd_peer_stream_to_peer_client(
        &mut self,
        msg: any_base::anychannel::RecvError<PeerStreamTunnelData>,
    ) -> Result<()> {
        if msg.is_err() {
            return Ok(());
        }

        let PeerStreamTunnelData {
            stream_id,
            typ,
            pack,
        } = msg.unwrap();
        if typ == TunnelHeaderType::TunnelAddConnect {
            if self.peer_stream_connect.is_none() {
                log::error!("self.peer_stream_connect.is_none");
                return Ok(());
            }

            let peer_stream_len = self
                .peer_client_sender
                .peer_stream_len
                .load(Ordering::Relaxed);
            let peer_stream_max_len = self.peer_stream_max_len.load(Ordering::Relaxed) as i32;

            if peer_stream_len >= peer_stream_max_len {
                let peer_client_sender = &self.peer_client_sender;
                let max_connect = Arc::new(TunnelMaxConnect {
                    peer_stream_size: peer_stream_max_len as usize,
                });
                peer_client_sender
                    .stream_pack_to_peer_stream_tx
                    .send(TunnelArcPack::TunnelMaxConnect(max_connect))
                    .await?;
                return Ok(());
            }
            log::debug!(
                "peer_stream_len:{}, peer_stream_max_len:{}",
                peer_stream_len,
                peer_stream_max_len
            );
            let _ = self.arc_async.add_peer_stream().await;

            return Ok(());
        } else if typ == TunnelHeaderType::TunnelMaxConnect {
            self.arc_async
                .is_max_peer_stream
                .store(true, Ordering::Relaxed);
            return Ok(());
        }

        #[cfg(feature = "anydebug")]
        {
            match pack.clone() {
                TunnelArcPack::TunnelData(value) => {
                    if value.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                        log::info!("peer_client read pack_id:{}", value.header.pack_id);
                    }
                }
                _ => {}
            };
        }
        let ret = self.stream_pack_tx_map.send(stream_id, pack).await;
        match ret {
            AnyAsyncSenderErr::Err(e) => {
                log::debug!("stream_pack_tx_map:{}", e);
                return Ok(());
            }
            AnyAsyncSenderErr::Ok => {
                return Ok(());
            }
            AnyAsyncSenderErr::Close(_) => {}
            AnyAsyncSenderErr::None(pack) => {
                if self.accept_tx.is_some() {
                    log::debug!("accept_tx send");
                    let stream = self
                        .register_stream(stream_id)
                        .await
                        .map_err(|e| anyhow!("err:register_stream => e:{}", e))?;
                    self.accept_tx
                        .as_ref()
                        .unwrap()
                        .send(stream)
                        .await
                        .map_err(|_| anyhow!("err:accept_tx"))?;
                    self.stream_pack_tx_map.send(stream_id, pack).await;
                    return Ok(());
                }
            }
        }

        if typ != TunnelHeaderType::TunnelClose {
            let peer_client_sender = &self.peer_client_sender;
            let close_pack = Arc::new(TunnelClose { stream_id });
            peer_client_sender
                .stream_pack_to_peer_stream_tx
                .send(TunnelArcPack::TunnelClose(close_pack))
                .await?;
        }
        Ok(())
    }

    pub async fn cmd_register_stream(
        &mut self,
        cmd: (
            u32,
            mpsc::UnboundedSender<(PeerClientToStreamPackReceiver, SocketAddr, SocketAddr)>,
        ),
    ) -> Result<()> {
        let (stream_id, tx) = cmd;
        let peer_client_to_stream_pack_rx = self
            .stream_pack_tx_map
            .channel(DEFAULT_STREAM_CHANNEL_SIZE, stream_id);
        tx.send((
            peer_client_to_stream_pack_rx,
            self.local_addr.clone(),
            self.remote_addr.clone(),
        ))
        .map_err(|_| anyhow!("err:cmd_register_stream"))?;
        Ok(())
    }

    pub async fn client_insert_peer_stream(
        is_spawn: bool,
        peer_client_sender: &PeerClientSender,
        session_id: String,
        mut stream: StreamFlow,
    ) -> Result<()> {
        let hello_pack = TunnelHello {
            version: TUNNEL_VERSION.to_string(),
            session_id,
        };
        log::debug!("client hello_pack:{:?}", hello_pack);
        let (_, mut w) = tokio::io::split(&mut stream);
        protopack::write_pack(
            &mut w,
            protopack::TunnelHeaderType::TunnelHello,
            &hello_pack,
            true,
        )
        .await
        .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;

        PeerClient::insert_peer_stream(is_spawn, peer_client_sender, stream)
            .await
            .map_err(|e| anyhow!("err:insert_peer_stream => e:{}", e))?;
        Ok(())
    }

    pub async fn insert_peer_stream(
        is_spawn: bool,
        peer_client_sender: &PeerClientSender,
        stream: StreamFlow,
    ) -> Result<()> {
        let peer_stream_to_peer_client_tx =
            peer_client_sender.peer_stream_to_peer_client_tx.clone();
        let peer_stream_len = peer_client_sender.peer_stream_len.clone();
        let stream_pack_to_peer_stream_rx =
            peer_client_sender.stream_pack_to_peer_stream_rx.clone();
        PeerStream::start(
            peer_stream_len,
            is_spawn,
            stream,
            peer_stream_to_peer_client_tx,
            stream_pack_to_peer_stream_rx,
        )
        .await
        .map_err(|e| anyhow!("err:PeerStream::start => e:{}", e))?;
        Ok(())
    }

    pub async fn async_register_stream(
        peer_client_sender: &PeerClientSender,
        stream_id: u32,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        let (wait_tx, mut wait_rx) = mpsc::unbounded_channel();
        peer_client_sender
            .peer_client_cmd_tx
            .send(PeerClientCmd::RegisterStream((stream_id, wait_tx)))
            .map_err(|_| anyhow!("err:RegisterStream"))?;
        let msg = wait_rx.recv().await;
        if msg.is_none() {
            return Err(anyhow!("err:RegisterStream"))?;
        }
        let (peer_client_to_stream_pack_rx, local_addr, remote_addr) = msg.unwrap();
        let stream_pack_to_peer_stream_tx =
            peer_client_sender.stream_pack_to_peer_stream_tx.clone();
        PeerClient::insert_stream(
            stream_pack_to_peer_stream_tx,
            stream_id,
            local_addr,
            remote_addr,
            peer_client_to_stream_pack_rx,
        )
        .await
    }

    pub async fn register_stream(
        &mut self,
        stream_id: u32,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        log::debug!("accept_map stream_id:{:?}", stream_id);
        let peer_client_sender = &self.peer_client_sender;
        let local_addr = self.local_addr.clone();
        let remote_addr = self.remote_addr.clone();
        let peer_client_to_stream_pack_rx = self
            .stream_pack_tx_map
            .channel(DEFAULT_STREAM_CHANNEL_SIZE, stream_id);
        let stream_pack_to_peer_stream_tx =
            peer_client_sender.stream_pack_to_peer_stream_tx.clone();
        PeerClient::insert_stream(
            stream_pack_to_peer_stream_tx,
            stream_id,
            local_addr,
            remote_addr,
            peer_client_to_stream_pack_rx,
        )
        .await
    }

    pub async fn insert_stream(
        stream_pack_to_peer_stream_tx: async_channel::Sender<TunnelArcPack>,
        stream_id: u32,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        peer_client_to_stream_pack_rx: PeerClientToStreamPackReceiver,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        let (stream_to_stream_pack_tx, stream_to_stream_pack_rx) =
            async_channel::bounded(DEFAULT_STREAM_CHANNEL_SIZE);
        let (stream_pack_to_stream_tx, stream_pack_to_stream_rx) =
            StreamPackToStreamChannel::channel(DEFAULT_STREAM_CHANNEL_SIZE);

        let async_ = Box::pin(async move {
            let stream_pack = StreamPack::new(stream_id, stream_pack_to_peer_stream_tx);
            stream_pack
                .start(
                    stream_pack_to_stream_tx,
                    peer_client_to_stream_pack_rx,
                    stream_to_stream_pack_rx,
                )
                .await
        });

        if cfg!(feature = "anyruntime-tokio-spawn-local") {
            any_base::executor_local_spawn::_start_and_free(move || async move { async_.await });
        } else {
            any_base::executor_spawn::_start_and_free(move || async move { async_.await });
        }

        Ok((
            Stream::new(stream_to_stream_pack_tx, stream_pack_to_stream_rx),
            local_addr,
            remote_addr,
        ))
    }
}
