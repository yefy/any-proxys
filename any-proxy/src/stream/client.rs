use super::stream_flow;
use crate::Protocol7;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

#[async_trait]
pub trait Client: Send + Sync + 'static {
    async fn connect(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<Box<dyn Connection + Send>>;
}

#[async_trait]
pub trait Connection: Send + Sync + 'static {
    async fn stream(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)>;
}
