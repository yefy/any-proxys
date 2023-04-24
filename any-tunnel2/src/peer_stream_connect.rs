use super::stream_flow::StreamFlow;
use super::Protocol4;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

#[async_trait]
pub trait PeerStreamConnect: Send + Sync {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)>;
    async fn addr(&self) -> Result<SocketAddr>; //这个是域名获取到的ip地址
    async fn host(&self) -> String; //地址或域名  配上文件上的
    async fn protocol4(&self) -> Protocol4; //tcp 或 udp  四层协议
    async fn protocol7(&self) -> String; //tcp udp  quic http 等7层协议
    async fn stream_send_timeout(&self) -> usize;
    async fn stream_recv_timeout(&self) -> usize;
    async fn is_tls(&self) -> bool;
}
