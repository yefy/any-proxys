use super::anychannel::AnyAsyncReceiver;
use super::protopack::TunnelArcPack;
use super::protopack::TunnelClose;
use super::protopack::TunnelData;
use super::protopack::TunnelDataAck;
use super::protopack::TunnelDataAckData;
use super::protopack::TunnelDataAckHeader;
use super::protopack::TunnelDataHeader;
use super::DEFAULT_CLOSE_TIMEOUT;
use super::DEFAULT_HEADBEAT_TIMEOUT;
#[cfg(feature = "anydebug")]
use super::DEFAULT_PRINT_NUM;
use super::DEFAULT_WINDOW_LEN;
use crate::anychannel::AnyAsyncSender;
use crate::protopack::TunnelHeartbeat;
use crate::{PeerClientToStreamPackReceiver, StreamPackToStreamSender, DEFAULT_MERGE_ACK_SIZE};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use chrono::prelude::*;
use std::collections::HashMap;
use std::collections::LinkedList;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use tokio::sync::Mutex;

#[derive(Clone)]
struct SendWindowWait {
    max_send_pack_len: Arc<AtomicUsize>,
    curr_send_pack_len: Arc<AtomicUsize>,
    waker: ArcMutex<Waker>,
    curr_pack_id: Arc<AtomicU32>,
}

impl SendWindowWait {
    pub fn new() -> SendWindowWait {
        SendWindowWait {
            max_send_pack_len: Arc::new(AtomicUsize::new(DEFAULT_WINDOW_LEN)),
            curr_send_pack_len: Arc::new(AtomicUsize::new(0)),
            waker: ArcMutex::default(),
            curr_pack_id: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn add_window_size(&self, add_window_size: usize, pack_id: u32) {
        self.curr_pack_id.store(pack_id, Ordering::Relaxed);
        self.curr_send_pack_len
            .fetch_add(add_window_size, Ordering::Relaxed);
    }

    pub fn waker(&self, sub_window_size: usize) {
        self.curr_send_pack_len
            .fetch_sub(sub_window_size, Ordering::Relaxed);

        let waker = unsafe { self.waker.take() };
        if waker.is_some() {
            waker.unwrap().wake();
        }
    }
}

impl Future for SendWindowWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let curr_send_pack_len = self.curr_send_pack_len.load(Ordering::Relaxed);
        let max_send_pack_len = self.max_send_pack_len.load(Ordering::Relaxed);
        if curr_send_pack_len >= max_send_pack_len {
            self.waker.set(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

pub struct WaitingPackInfo {
    pub insert_timestamp: i64,
    pub first_send_timestamp: i64,
    pub pack_id: u32,
}

pub struct StreamPack {
    stream_id: u32,
    stream_pack_to_peer_stream_tx: async_channel::Sender<TunnelArcPack>,
    waiting_pack_map: Mutex<HashMap<u32, (TunnelArcPack, i64)>>,
    waiting_pack_infos: Mutex<LinkedList<WaitingPackInfo>>,
    send_window_wait: SendWindowWait,
    curr_timestamp_millis: AtomicI64,
    rtt_timestamp_millis: AtomicI64,
    ack_resp_timestamp_millis: AtomicI64,
    is_close: Arc<AtomicBool>,
}

impl StreamPack {
    pub fn new(
        stream_id: u32,
        stream_pack_to_peer_stream_tx: async_channel::Sender<TunnelArcPack>,
    ) -> StreamPack {
        let curr_timestamp_millis = Local::now().timestamp_millis();
        StreamPack {
            stream_id,
            stream_pack_to_peer_stream_tx,
            waiting_pack_map: Mutex::new(HashMap::new()),
            waiting_pack_infos: Mutex::new(LinkedList::new()),
            send_window_wait: SendWindowWait::new(),
            curr_timestamp_millis: AtomicI64::new(curr_timestamp_millis),
            rtt_timestamp_millis: AtomicI64::new(50),
            ack_resp_timestamp_millis: AtomicI64::new(curr_timestamp_millis),
            is_close: Arc::new(AtomicBool::new(false)),
        }
    }
    pub async fn start(
        &self,
        stream_pack_to_stream_tx: StreamPackToStreamSender,
        peer_client_to_stream_pack_rx: PeerClientToStreamPackReceiver,
        mut stream_to_stream_pack_rx: async_channel::Receiver<Vec<u8>>,
    ) -> Result<()> {
        let is_write_break = Arc::new(AtomicBool::new(false));
        let ret: Result<()> = async {
            tokio::select! {
                biased;
                ret = self.peer_client_to_stream_pack(stream_pack_to_stream_tx, peer_client_to_stream_pack_rx, is_write_break.clone()) => {
                    ret.map_err(|e| anyhow!("err:peer_client_to_stream_pack => e:{}", e))?;
                    Ok(())
                }
                ret = self.stream_to_stream_pack(&mut stream_to_stream_pack_rx) => {
                    ret.map_err(|e| anyhow!("err:stream_to_stream_pack => e:{}", e))?;
                    Ok(())
                }
                ret = self.check_waiting_pack_timeout() => {
                    ret.map_err(|e| anyhow!("err:check_waiting_pack_timeout => e:{}", e))?;
                    Ok(())
                }
                ret = self.heartbeat() => {
                    ret.map_err(|e| anyhow!("err:heartbeat => e:{}", e))?;
                    Ok(())
                }
                _ = self.curr_timestamp_millis() => {
                    Ok(())
                }
                _ = self.is_write_break(is_write_break) => {
                    Ok(())
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        }
            .await;
        log::debug!(target: "main", "send_pack_close");
        if let Err(_) = self.send_pack_close().await {
            log::debug!(target: "main", "send_pack_close err");
        }
        ret?;
        Ok(())
    }

    async fn peer_client_to_stream_pack(
        &self,
        stream_pack_to_stream_tx: StreamPackToStreamSender,
        peer_client_to_stream_pack_rx: PeerClientToStreamPackReceiver,
        is_write_break: Arc<AtomicBool>,
    ) -> Result<()> {
        let mut pack_id = 1u32;
        let mut send_pack_id_map = HashMap::<u32, ()>::new();
        let mut recv_pack_cache_map = HashMap::<u32, Arc<TunnelData>>::new();
        loop {
            if self.is_close.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
            let ret: Result<()> = async {
                let mut pack_ids = Vec::with_capacity(10);
                let mut merge_ack_size = DEFAULT_MERGE_ACK_SIZE;
                let tunnel_data: Result<Option<Arc<TunnelData>>> = async {
                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(1),
                        peer_client_to_stream_pack_rx.recv(),
                    )
                    .await
                    {
                        Ok(pack) => {
                            //let pack = peer_client_to_stream_pack_rx.recv().await;
                            if pack.is_err() {
                                log::debug!(target: "main", "peer_client_to_stream_pack close");
                                return Err(anyhow!("err:peer_client_to_stream_pack_rx close"));
                            }
                            let pack = pack.unwrap();
                            match pack {
                                TunnelArcPack::TunnelHello(value) => {
                                    log::error!("err:TunnelHello => TunnelHello:{:?}", value);
                                    return Err(anyhow!(
                                        "err:TunnelHello => TunnelHello:{:?}",
                                        value
                                    ));
                                }
                                TunnelArcPack::TunnelAddConnect(value) => {
                                    log::error!("err:TunnelAddConnect => value:{:?}", value);
                                    return Err(anyhow!(
                                        "err:TunnelAddConnect => value:{:?}",
                                        value
                                    ));
                                }
                                TunnelArcPack::TunnelMaxConnect(value) => {
                                    log::error!("err:TunnelMaxConnect => value:{:?}", value);
                                    return Err(anyhow!(
                                        "err:TunnelMaxConnect => value:{:?}",
                                        value
                                    ));
                                }
                                TunnelArcPack::TunnelData(tunnel_data) => {
                                    if tunnel_data.header.stream_id != self.stream_id {
                                        log::error!("err:value.header.stream_id != self.stream_id");
                                        return Err(anyhow!(
                                            "err:value.header.stream_id != self.stream_id"
                                        ));
                                    }

                                    #[cfg(feature = "anydebug")]
                                    {
                                        if tunnel_data.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                                            log::info!(
                                                "stream_pack read pack_id:{}, local pack_id:{}",
                                                tunnel_data.header.pack_id,
                                                pack_id
                                            );
                                        }
                                    }

                                    if send_pack_id_map.get(&tunnel_data.header.pack_id).is_some() {
                                        pack_ids.push(tunnel_data.header.pack_id);
                                        return Ok(None);
                                    }
                                    if tunnel_data.header.pack_id == pack_id {
                                        Ok(Some(tunnel_data))
                                    } else {
                                        recv_pack_cache_map
                                            .insert(tunnel_data.header.pack_id, tunnel_data);
                                        Ok(None)
                                    }
                                }
                                TunnelArcPack::TunnelDataAck(value) => {
                                    if value.header.stream_id != self.stream_id {
                                        log::error!("err:stream_id != self.stream_id");
                                        return Err(anyhow!("err:stream_id != self.stream_id"));
                                    }

                                    let mut first_send_time = 0;
                                    {
                                        let mut waiting_pack_map =
                                            self.waiting_pack_map.lock().await;
                                        for pack_id in value.data.pack_ids.iter() {
                                            log::trace!(target: "main", "pack_id drop:{:?}", pack_id);
                                            #[cfg(feature = "anydebug")]
                                            {
                                                if pack_id % DEFAULT_PRINT_NUM == 0 {
                                                    log::info!(
                                                        "stream_pack ack pack_id:{}",
                                                        pack_id
                                                    );
                                                }
                                            }

                                            let pack = waiting_pack_map.remove(&pack_id);
                                            if pack.is_none() {
                                                continue;
                                            }
                                            let (pack, _first_send_time) = pack.unwrap();
                                            if first_send_time == 0 {
                                                first_send_time = _first_send_time;
                                            }
                                            match pack {
                                                TunnelArcPack::TunnelData(data) => {
                                                    self.send_window_wait.waker(data.datas.len());
                                                }
                                                _ => {}
                                            }
                                        }
                                    }

                                    if first_send_time > 0 {
                                        let curr_timestamp_millis =
                                            self.curr_timestamp_millis.load(Ordering::Relaxed);
                                        let rtt_timestamp_millis =
                                            curr_timestamp_millis - first_send_time;
                                        self.rtt_timestamp_millis
                                            .store(rtt_timestamp_millis, Ordering::Relaxed);
                                        self.ack_resp_timestamp_millis
                                            .store(curr_timestamp_millis, Ordering::Relaxed);
                                    }
                                    Ok(None)
                                }
                                TunnelArcPack::TunnelClose(value) => {
                                    if value.stream_id != self.stream_id {
                                        log::error!("err:TunnelClose.stream_id != self.stream_id");
                                        return Err(anyhow!(
                                            "err:TunnelClose.stream_id != self.stream_id"
                                        ));
                                    }
                                    return Err(anyhow!("err:TunnelClose"))?;
                                }
                                TunnelArcPack::TunnelHeartbeat(value) => {
                                    if value.stream_id != self.stream_id {
                                        log::error!("err:TunnelClose.stream_id != self.stream_id");
                                        return Err(anyhow!(
                                            "err:TunnelClose.stream_id != self.stream_id"
                                        ));
                                    }

                                    self.send_heartbeat_ack(value).await.map_err(|e| {
                                        anyhow!("err:send_heartbeat_ack => e:{}", e)
                                    })?;
                                    Ok(None)
                                }
                                TunnelArcPack::TunnelHeartbeatAck(value) => {
                                    if value.stream_id != self.stream_id {
                                        log::error!("err:TunnelClose.stream_id != self.stream_id");
                                        return Err(anyhow!(
                                            "err:TunnelClose.stream_id != self.stream_id"
                                        ));
                                    }

                                    let curr_timestamp_millis =
                                        self.curr_timestamp_millis.load(Ordering::Relaxed);
                                    self.ack_resp_timestamp_millis
                                        .store(curr_timestamp_millis, Ordering::Relaxed);
                                    Ok(None)
                                }
                            }
                        }
                        Err(_) => Ok(None),
                    }
                }
                .await;

                let mut tunnel_data = tunnel_data?;
                is_write_break.store(false, Ordering::Relaxed);
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
                    pack_ids.push(tunnel_data.header.pack_id);

                    //ack 够了先发送
                    if pack_ids.len() >= merge_ack_size {
                        self.send_tunnel_data_ack(self.stream_id, pack_ids.clone())
                            .await
                            .map_err(|e| anyhow!("err:send_tunnel_data_ack => e:{}", e))?;
                        pack_ids.clear();
                    }
                    log::trace!(target: "main", "tunnel_data.header:{:?}", tunnel_data.header);

                    //尝试发送判断队列是否满了， 如果满了ack就不合并了马上发送， 为了在阻塞前windows窗口最大化
                    if let Err(e) = stream_pack_to_stream_tx.try_send(tunnel_data) {
                        let tunnel_data = match e {
                            async_channel::TrySendError::Full(tunnel_data) => {
                                merge_ack_size = 1;
                                tunnel_data
                            }
                            _ => break,
                        };

                        if pack_ids.len() >= merge_ack_size {
                            self.send_tunnel_data_ack(self.stream_id, pack_ids.clone())
                                .await
                                .map_err(|e| anyhow!("err:send_tunnel_data_ack => e:{}", e))?;
                            pack_ids.clear();
                        }

                        if let Err(_) = stream_pack_to_stream_tx.send(tunnel_data).await {
                            log::debug!(target: "main", "stream_pack_to_stream_tx close");
                            break;
                        }
                    } else {
                        merge_ack_size = DEFAULT_MERGE_ACK_SIZE;
                    }

                    if is_write_break.load(Ordering::Relaxed) {
                        break;
                    }
                }

                if pack_ids.len() > 0 {
                    self.send_tunnel_data_ack(self.stream_id, pack_ids)
                        .await
                        .map_err(|e| anyhow!("err:send_tunnel_data_ack => e:{}", e))?;
                }
                Ok(())
            }
            .await;
            if ret.is_err() {
                log::debug!(target: "main", "peer_client_to_stream_pack close");
                self.is_close.swap(true, Ordering::Relaxed);
            }
        }
    }

    async fn stream_to_stream_pack(
        &self,
        stream_to_stream_pack_rx: &mut async_channel::Receiver<Vec<u8>>,
    ) -> Result<()> {
        let mut pack_id = 0;
        loop {
            let ret: Result<()> = async {
                #[cfg(feature = "anyack")]
                self.send_window_wait.clone().await;

                let datas = match tokio::time::timeout(
                    tokio::time::Duration::from_secs(1),
                    stream_to_stream_pack_rx.recv(),
                )
                .await
                {
                    Ok(datas) => datas
                        .map_err(|e| anyhow!("err:stream_to_stream_pack_rx recv => e:{}", e))?,
                    Err(_) => {
                        return Ok(());
                    }
                };
                #[cfg(feature = "anydebug")]
                {
                    if pack_id % DEFAULT_PRINT_NUM == 0 {
                        log::info!("stream_pack write pack_id:{}", pack_id);
                    }
                }
                self.send_window_wait.add_window_size(datas.len(), pack_id);
                pack_id += 1;
                let header = TunnelDataHeader {
                    stream_id: self.stream_id,
                    pack_id,
                    pack_size: datas.len() as u32,
                };
                log::trace!(target: "main", "stream_pack_to_peer_client:{:?}", header);
                let value = Arc::new(TunnelData { header, datas });
                self.stream_pack_to_peer_client(pack_id, TunnelArcPack::TunnelData(value), 0)
                    .await
                    .map_err(|e| anyhow!("err:stream_pack_to_peer_client => e:{}", e))?;
                Ok(())
            }
            .await;
            if let Err(e) = ret {
                log::debug!(target: "main", "stream_to_stream_pack_rx close, e:{}", e);
                return Ok(());
            }

            if self.is_close.load(Ordering::Relaxed) {
                log::debug!(target: "main", "is_close stream_to_stream_pack_rx close");
                return Ok(());
            }
        }
    }

    async fn check_waiting_pack_timeout(&self) -> Result<()> {
        let delay_multiple = 3;
        loop {
            let rtt_timestamp_millis = self.rtt_timestamp_millis.load(Ordering::Relaxed);
            let rtt_timestamp_millis = if rtt_timestamp_millis <= 1000 {
                1000
            } else {
                rtt_timestamp_millis as u64
            };
            tokio::time::sleep(tokio::time::Duration::from_millis(
                rtt_timestamp_millis * delay_multiple,
            ))
            .await;

            if self.is_close.load(Ordering::Relaxed) {
                continue;
            }

            #[cfg(not(feature = "anyack"))]
            if true {
                continue;
            }

            let curr_timestamp_millis = self.curr_timestamp_millis.load(Ordering::Relaxed);
            loop {
                let waiting_pack_info = {
                    let mut waiting_pack_infos = self.waiting_pack_infos.lock().await;
                    let waiting_pack_info = waiting_pack_infos.front();
                    if waiting_pack_info.is_none() {
                        break;
                    }
                    let waiting_pack_info = waiting_pack_info.unwrap();
                    if curr_timestamp_millis - waiting_pack_info.insert_timestamp
                        > (rtt_timestamp_millis * delay_multiple) as i64
                    {
                        waiting_pack_infos.pop_front()
                    } else {
                        break;
                    }
                };

                if waiting_pack_info.is_none() {
                    break;
                }
                let waiting_pack_info = waiting_pack_info.unwrap();
                let waiting_pack = {
                    self.waiting_pack_map
                        .lock()
                        .await
                        .remove(&waiting_pack_info.pack_id)
                };
                if waiting_pack.is_none() {
                    continue;
                }
                let (waiting_pack, first_send_time) = waiting_pack.unwrap();

                #[cfg(feature = "anydebug")]
                log::info!(
                    "timeout waiting_pack_info.pack_id:{}",
                    waiting_pack_info.pack_id
                );

                self.stream_pack_to_peer_client(
                    waiting_pack_info.pack_id,
                    waiting_pack,
                    first_send_time,
                )
                .await
                .map_err(|e| anyhow!("err:stream_pack_to_peer_client => e:{}", e))?;
            }
        }
    }

    async fn heartbeat(&self) -> Result<()> {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            #[cfg(feature = "anydebug")]
            {
                log::info!(
                    "waiting_pack_map len:{}",
                    self.waiting_pack_map.lock().await.len()
                );
            }

            let curr_timestamp_millis = self.curr_timestamp_millis.load(Ordering::Relaxed);
            let ack_resp_timestamp_millis = self.ack_resp_timestamp_millis.load(Ordering::Relaxed);
            let diff_timestamp_millis = curr_timestamp_millis - ack_resp_timestamp_millis;
            if diff_timestamp_millis >= DEFAULT_CLOSE_TIMEOUT {
                log::error!("heartbeat timeout");
                return Ok(());
            }

            if diff_timestamp_millis < DEFAULT_HEADBEAT_TIMEOUT {
                continue;
            }

            self.send_heartbeat()
                .await
                .map_err(|e| anyhow!("err:send_heartbeat => e:{}", e))?;
        }
    }

    async fn send_pack_close(&self) -> Result<()> {
        let value = Arc::new(TunnelClose {
            stream_id: self.stream_id.clone(),
        });
        self.stream_pack_to_peer_stream_tx
            .send(TunnelArcPack::TunnelClose(value))
            .await?;
        Ok(())
    }

    async fn send_tunnel_data_ack(&self, stream_id: u32, pack_ids: Vec<u32>) -> Result<()> {
        #[cfg(not(feature = "anyack"))]
        if true {
            return Ok(());
        }

        let tunnel_data_ack = TunnelDataAck {
            header: TunnelDataAckHeader {
                stream_id,
                data_size: 0,
            },
            data: TunnelDataAckData { pack_ids },
        };
        let _ = self
            .stream_pack_to_peer_stream_tx
            .send(TunnelArcPack::TunnelDataAck(Arc::new(tunnel_data_ack)))
            .await;
        Ok(())
    }

    async fn send_heartbeat(&self) -> Result<()> {
        let value = Arc::new(TunnelHeartbeat {
            stream_id: self.stream_id,
            time: self.curr_timestamp_millis.load(Ordering::Relaxed),
        });

        self.stream_pack_to_peer_stream_tx
            .send(TunnelArcPack::TunnelHeartbeat(value))
            .await?;

        Ok(())
    }

    async fn send_heartbeat_ack(&self, value: Arc<TunnelHeartbeat>) -> Result<()> {
        let value = Arc::new(TunnelHeartbeat {
            stream_id: value.stream_id,
            time: value.time,
        });

        self.stream_pack_to_peer_stream_tx
            .send(TunnelArcPack::TunnelHeartbeatAck(value))
            .await?;

        Ok(())
    }

    async fn stream_pack_to_peer_client(
        &self,
        pack_id: u32,
        pack: TunnelArcPack,
        first_send_time: i64,
    ) -> Result<()> {
        let curr_timestamp = self.curr_timestamp_millis.load(Ordering::Relaxed);
        let first_send_time = if first_send_time > 0 {
            first_send_time
        } else {
            curr_timestamp
        };
        {
            self.waiting_pack_infos
                .lock()
                .await
                .push_back(WaitingPackInfo {
                    insert_timestamp: curr_timestamp,
                    first_send_timestamp: first_send_time,
                    pack_id: pack_id,
                });

            self.waiting_pack_map
                .lock()
                .await
                .insert(pack_id, (pack.clone(), first_send_time));
        }

        self.stream_pack_to_peer_stream_tx.send(pack).await?;
        Ok(())
    }

    pub async fn curr_timestamp_millis(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            self.curr_timestamp_millis
                .store(Local::now().timestamp_millis(), Ordering::Relaxed);
        }
    }

    pub async fn is_write_break(&self, is_write_break: Arc<AtomicBool>) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            is_write_break.store(true, Ordering::Relaxed)
        }
    }
}
