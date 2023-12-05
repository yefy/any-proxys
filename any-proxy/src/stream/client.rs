use super::stream_flow;
use crate::Protocol7;
use any_base::stream_flow::StreamFlowInfo;
use any_base::typ::ArcMutex;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

#[async_trait]
pub trait Client: Send + Sync + 'static {
    async fn connect(&self, info: ArcMutex<StreamFlowInfo>) -> Result<Box<dyn Connection + Send>>;
}

#[async_trait]
pub trait Connection: Send + Sync + 'static {
    async fn stream(
        &self,
        info: ArcMutex<StreamFlowInfo>,
    ) -> Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)>;
}
