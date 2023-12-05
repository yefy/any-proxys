use crate::Protocol7;
use any_base::executor_local_spawn::Runtime;
use any_base::stream_flow::{StreamFlow, StreamFlowInfo};
use any_base::typ::ArcMutex;
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

#[async_trait]
pub trait Connect: Send + Sync {
    async fn connect(
        &self,
        request_id: Option<String>,
        stream_info: ArcMutex<StreamFlowInfo>,
        run_time: Option<Arc<Box<dyn Runtime>>>,
    ) -> Result<(StreamFlow, ConnectInfo)>;
    async fn addr(&self) -> Result<SocketAddr>;
    async fn host(&self) -> Result<String>;
    async fn is_tls(&self) -> bool;
    async fn protocol7(&self) -> String;
    async fn domain(&self) -> String;
}
