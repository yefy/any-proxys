use super::stream_flow;
use crate::Protocol7;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

#[async_trait(?Send)]
pub trait Connect {
    async fn connect(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(
        Protocol7,
        stream_flow::StreamFlow,
        String,
        f32,
        SocketAddr,
        SocketAddr,
    )>;
}
