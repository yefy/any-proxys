use super::stream_flow;
use crate::Protocol7;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

pub struct ConnectInfo {
    pub protocol7: Protocol7,
    pub domain: String,
    pub elapsed: f32,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub peer_stream_size: Option<Arc<AtomicUsize>>,
    pub max_stream_size: Option<usize>,
    pub min_stream_cache_size: Option<usize>,
    pub channel_size: Option<usize>,
}

#[async_trait(?Send)]
pub trait Connect {
    async fn connect(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)>;
}
