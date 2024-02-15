use super::stream_flow::StreamFlow;
use super::Protocol4;
use any_base::stream_flow::StreamFlow;
use any_tunnel2::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;

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

pub struct PeerStreamConnectTcp {
    addr: String,
}

impl PeerStreamConnectTcp {
    pub fn new(addr: String) -> PeerStreamConnectTcp {
        PeerStreamConnectTcp { addr }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let stream = TcpStream::connect(self.addr().await?)
            .await
            .map_err(|e| anyhow!("err:TcpStream::connect => e:{}", e))?;
        let local_addr = stream.local_addr()?;
        let remote_addr = stream.peer_addr()?;
        Ok((StreamFlow::new_tokio(stream), local_addr, remote_addr))
    }
    async fn addr(&self) -> Result<SocketAddr> {
        let connect_addr =
            self.addr.to_socket_addrs()?.next().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "err:empty address")
            })?;
        Ok(connect_addr)
    }

    async fn host(&self) -> String {
        self.addr.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }
    async fn protocol7(&self) -> String {
        "tcp".to_string()
    }
    async fn stream_send_timeout(&self) -> usize {
        10
    }
    async fn stream_recv_timeout(&self) -> usize {
        10
    }

    async fn is_tls(&self) -> bool {
        false
    }
}
