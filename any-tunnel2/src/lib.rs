pub mod client;
pub mod peer_client;
pub mod peer_stream;
pub mod peer_stream_connect;
pub mod protopack;
pub mod rt;
pub mod server;
pub mod stream;
pub mod stream_pack;
pub mod tunnel;

use crate::anychannel::{AnyChannel, AnyReceiver, AnySender};
#[cfg(feature = "anytunnel2-ack")]
use crate::anychannel::{AnyUnboundedChannel, AnyUnboundedReceiver};
use crate::protopack::{TunnelArcPack, TunnelData, TunnelHeaderType};
pub use any_base::anychannel;
pub use any_base::stream_flow;
use std::sync::Arc;

#[cfg(not(feature = "anytunnel2-ack"))]
pub type PeerClientToStreamPackChannel = AnyChannel<TunnelArcPack>;
#[cfg(not(feature = "anytunnel2-ack"))]
pub type PeerClientToStreamPackReceiver = AnyReceiver<TunnelArcPack>;

#[cfg(feature = "anytunnel2-ack")]
pub type PeerClientToStreamPackChannel = AnyUnboundedChannel<TunnelArcPack>;
#[cfg(feature = "anytunnel2-ack")]
pub type PeerClientToStreamPackReceiver = AnyUnboundedReceiver<TunnelArcPack>;

#[derive(Clone)]
pub struct PeerStreamTunnelData {
    stream_id: u32,
    typ: TunnelHeaderType,
    pack: TunnelArcPack,
}

impl PeerStreamTunnelData {
    pub fn new(stream_id: u32, typ: TunnelHeaderType, pack: TunnelArcPack) -> PeerStreamTunnelData {
        PeerStreamTunnelData {
            stream_id,
            typ,
            pack,
        }
    }
}

pub type PeerStreamToPeerClientChannel = AnyChannel<PeerStreamTunnelData>;
pub type PeerStreamToPeerClientReceiver = AnyReceiver<PeerStreamTunnelData>;
pub type PeerStreamToPeerClientSender = AnySender<PeerStreamTunnelData>;

pub type StreamPackToStreamChannel = AnyChannel<Arc<TunnelData>>;
pub type StreamPackToStreamReceiver = AnyReceiver<Arc<TunnelData>>;
pub type StreamPackToStreamSender = AnySender<Arc<TunnelData>>;

pub const DEFAULT_MERGE_ACK_SIZE: usize = 128;
pub const DEFAULT_STREAM_CHANNEL_SIZE: usize = 64;
pub const DEFAULT_WINDOW_LEN: usize = 1024 * 1024;
pub const DEFAULT_HEADBEAT_TIMEOUT: i64 = 1000 * 2;
pub const DEFAULT_CLOSE_TIMEOUT: i64 = 1000 * 10;

#[cfg(feature = "anytunnel2-debug")]
pub const DEFAULT_PRINT_NUM: u32 = 1000;

#[derive(Clone, Eq, PartialEq)]
pub enum Protocol4 {
    TCP,
    UDP,
}

impl Protocol4 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol4::TCP => "tcp".to_string(),
            Protocol4::UDP => "udp".to_string(),
        }
    }
}
