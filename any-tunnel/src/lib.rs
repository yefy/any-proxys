pub mod client;
pub mod peer_client;
pub mod peer_stream;
pub mod peer_stream_connect;
pub mod protopack;
pub mod round_async_channel;
pub mod server;
pub mod stream;

use crate::protopack::DynamicTunnelData;
pub use any_base::stream_flow;

#[cfg(feature = "anytunnel-debug")]
pub const DEFAULT_PRINT_NUM: u32 = 10000;

pub type PeerClientToStreamSender = async_channel::Sender<DynamicTunnelData>;
pub type PeerClientToStreamReceiver = async_channel::Receiver<DynamicTunnelData>;

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
