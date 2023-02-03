use super::protopack;
use super::protopack::TunnelArcPack;
use super::protopack::TunnelHeaderType;
use super::protopack::TunnelPack;
use super::stream_flow::StreamFlow;
use super::stream_flow::StreamFlowErr;
use super::stream_flow::StreamFlowInfo;
#[cfg(feature = "anytunnel2-debug")]
use super::DEFAULT_PRINT_NUM;
use crate::anychannel::AnyAsyncSender;
use crate::{PeerStreamToPeerClientSender, PeerStreamTunnelData};
use anyhow::anyhow;
use anyhow::Result;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

pub struct PeerStream {}

impl PeerStream {
    pub async fn start(
        peer_stream_len: Arc<AtomicI32>,
        is_spawn: bool,
        stream: StreamFlow,
        peer_stream_to_peer_client_tx: PeerStreamToPeerClientSender,
        pack_to_peer_stream_rx: async_channel::Receiver<TunnelArcPack>,
    ) -> Result<()> {
        if is_spawn {
            tokio::spawn(async move {
                let ret: Result<()> = async {
                    PeerStream::do_start(
                        peer_stream_len,
                        stream,
                        peer_stream_to_peer_client_tx,
                        pack_to_peer_stream_rx,
                    )
                    .await
                    .map_err(|e| anyhow!("err:PeerStream::do_start => e:{}", e))?;
                    Ok(())
                }
                .await;
                ret.unwrap_or_else(|e| log::error!("err:PeerStream::do_start => e:{}", e));
            });
        } else {
            PeerStream::do_start(
                peer_stream_len,
                stream,
                peer_stream_to_peer_client_tx,
                pack_to_peer_stream_rx,
            )
            .await
            .map_err(|e| anyhow!("err:PeerStream::do_start => e:{}", e))?;
        }
        Ok(())
    }

    pub async fn do_start(
        peer_stream_len: Arc<AtomicI32>,
        mut stream: StreamFlow,
        peer_stream_to_peer_client_tx: PeerStreamToPeerClientSender,
        mut pack_to_peer_stream_rx: async_channel::Receiver<TunnelArcPack>,
    ) -> Result<()> {
        peer_stream_len.fetch_add(1, Ordering::Relaxed);
        let stream_info = Arc::new(std::sync::Mutex::new(StreamFlowInfo::new()));
        stream.set_config(
            tokio::time::Duration::from_secs(60 * 10),
            tokio::time::Duration::from_secs(60 * 10),
            Some(stream_info.clone()),
        );
        let (r, w) = stream.split();
        let r = any_base::io::buf_reader::BufReader::new(r);
        let w = any_base::io::buf_writer::BufWriter::new(w);

        let ret: Result<()> = async {
            tokio::select! {
                biased;
                ret = PeerStream::read_stream(r,  &peer_stream_to_peer_client_tx) => {
                    ret.map_err(|e| anyhow!("err:peer_stream read_stream => e:{}", e))?;
                    Ok(())
                }
                ret = PeerStream::write_stream(w, &mut pack_to_peer_stream_rx) => {
                    ret.map_err(|e| anyhow!("err:peer_stream write_stream => e:{}", e))?;
                    Ok(())
                }
                else => {
                    return Err(anyhow!("err:upstream_pack.rs"));
                }
            }
        }
        .await;

        peer_stream_len.fetch_sub(1, Ordering::Relaxed);
        if let Err(e) = ret {
            let err = { stream_info.lock().unwrap().err.clone() };
            if err as i32 >= StreamFlowErr::WriteTimeout as i32 {
                return Err(e)?;
            }
        }
        Ok(())
    }

    async fn read_stream<R: AsyncRead + std::marker::Unpin>(
        mut r: R,
        peer_stream_to_peer_client_tx: &PeerStreamToPeerClientSender,
    ) -> Result<()> {
        let mut slice = [0u8; protopack::TUNNEL_MAX_HEADER_SIZE];
        loop {
            let pack = protopack::read_pack_all(&mut r, &mut slice)
                .await
                .map_err(|e| anyhow!("err:protopack::read_pack => e:{}", e))?;
            match pack {
                TunnelPack::TunnelHello(value) => {
                    log::error!("err:TunnelHello => TunnelHello:{:?}", value);
                }
                TunnelPack::TunnelData(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelData:{:?}", value.header);
                    #[cfg(feature = "anytunnel2-debug")]
                    {
                        if value.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                            log::info!("peer_stream read pack_id:{}", value.header.pack_id);
                        }
                    }

                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            value.header.stream_id,
                            TunnelHeaderType::TunnelData,
                            TunnelArcPack::TunnelData(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelDataAck(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelDataAck:{:?}", value);

                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            value.header.stream_id,
                            TunnelHeaderType::TunnelDataAck,
                            TunnelArcPack::TunnelDataAck(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelClose(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelClose:{:?}", value);
                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            value.stream_id,
                            TunnelHeaderType::TunnelClose,
                            TunnelArcPack::TunnelClose(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelHeartbeat(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelHeartbeat:{:?}", value);
                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            value.stream_id,
                            TunnelHeaderType::TunnelHeartbeat,
                            TunnelArcPack::TunnelHeartbeat(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelHeartbeatAck(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelHeartbeatAck:{:?}", value);
                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            value.stream_id,
                            TunnelHeaderType::TunnelHeartbeatAck,
                            TunnelArcPack::TunnelHeartbeatAck(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelAddConnect(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelCreateConnect:{:?}", value);
                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            0,
                            TunnelHeaderType::TunnelAddConnect,
                            TunnelArcPack::TunnelAddConnect(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
                TunnelPack::TunnelMaxConnect(value) => {
                    log::trace!("peer_stream_to_peer_client TunnelCreateConnect:{:?}", value);
                    peer_stream_to_peer_client_tx
                        .send(PeerStreamTunnelData::new(
                            0,
                            TunnelHeaderType::TunnelMaxConnect,
                            TunnelArcPack::TunnelMaxConnect(Arc::new(value)),
                        ))
                        .await
                        .map_err(|e| anyhow!("err:{}", e))?;
                }
            }
        }
    }

    async fn write_stream<W: AsyncWrite + std::marker::Unpin>(
        mut w: W,
        pack_to_peer_stream_rx: &mut async_channel::Receiver<TunnelArcPack>,
    ) -> Result<()> {
        loop {
            let pack = pack_to_peer_stream_rx
                .recv()
                .await
                .map_err(|_| anyhow!("pack_to_peer_stream_rx close"));
            if pack.is_err() {
                log::debug!("pack_to_peer_stream_rx close");
                return Ok(());
            }
            let pack = pack.unwrap();
            log::trace!("peer_client_to_stream write_stream");
            match pack {
                TunnelArcPack::TunnelHello(value) => {
                    log::error!("err:TunnelHello => TunnelHello:{:?}", value);
                }
                TunnelArcPack::TunnelData(value) => {
                    log::trace!("peer_stream_write TunnelData:{:?}", value.header);
                    #[cfg(feature = "anytunnel2-debug")]
                    {
                        if value.header.pack_id % DEFAULT_PRINT_NUM == 0 {
                            log::info!("peer_stream write pack_id:{}", value.header.pack_id);
                        }
                    }
                    protopack::write_tunnel_data(&mut w, &value)
                        .await
                        .map_err(|e| anyhow!("err:write_tunnel_data => e:{}", e))?;
                }
                TunnelArcPack::TunnelDataAck(value) => {
                    log::trace!("peer_stream_write TunnelDataAck:{:?}", value);
                    protopack::write_tunnel_data_ack(&mut w, &value)
                        .await
                        .map_err(|e| anyhow!("err:write_tunnel_data_ack => e:{}", e))?;
                }
                TunnelArcPack::TunnelClose(value) => {
                    log::trace!("peer_stream_write TunnelClose:{:?}", value);
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelClose,
                        value.as_ref(),
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;
                }
                TunnelArcPack::TunnelHeartbeat(value) => {
                    log::trace!("peer_stream_write TunnelHeartbeat:{:?}", value);
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelHeartbeat,
                        value.as_ref(),
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;
                }
                TunnelArcPack::TunnelHeartbeatAck(value) => {
                    log::trace!("peer_stream_write TunnelHeartbeatAck:{:?}", value);
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelHeartbeatAck,
                        value.as_ref(),
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;
                }
                TunnelArcPack::TunnelAddConnect(value) => {
                    log::trace!("peer_stream_write TunnelAddConnect:{:?}", value);
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelAddConnect,
                        value.as_ref(),
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;
                }
                TunnelArcPack::TunnelMaxConnect(value) => {
                    log::trace!("peer_stream_write TunnelMaxConnect:{:?}", value);
                    protopack::write_pack(
                        &mut w,
                        TunnelHeaderType::TunnelMaxConnect,
                        value.as_ref(),
                        true,
                    )
                    .await
                    .map_err(|e| anyhow!("err:protopack::write_pack => e:{}", e))?;
                }
            };
        }
    }
}
