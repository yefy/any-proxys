use super::stream_flow::StreamFlow;
use super::Protocol4;
use crate::stream::Stream;
use any_base::stream_flow::StreamFlow;
use any_tunnel::peer_stream_connect::PeerStreamConnect;
use any_tunnel::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;

pub struct PeerStreamConnectTcp {
    addr: String,
    max_stream_size: usize,
    min_stream_cache_size: usize,
    channel_size: usize,
}

impl PeerStreamConnectTcp {
    pub fn new(
        addr: String,
        max_stream_size: usize,
        min_stream_cache_size: usize,
        channel_size: usize,
    ) -> PeerStreamConnectTcp {
        PeerStreamConnectTcp {
            addr,
            max_stream_size,
            min_stream_cache_size,
            channel_size,
        }
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
        let (r, w) = tokio::io::split(stream);
        Ok((
            StreamFlow::new(0, Box::new(r), Box::new(w)),
            local_addr,
            remote_addr,
        ))
    }
    async fn addr(&self) -> Result<SocketAddr> {
        let connect_addr = self
            .addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "err:empty address"))
            .map_err(|e| anyhow!("err:to_socket_addrs => e:{}", e))?;
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
    async fn max_stream_size(&self) -> usize {
        self.max_stream_size
    }
    async fn min_stream_cache_size(&self) -> usize {
        self.min_stream_cache_size
    }
    async fn channel_size(&self) -> usize {
        self.channel_size
    }
    async fn stream_send_timeout(&self) -> usize {
        10
    }
    async fn stream_recv_timeout(&self) -> usize {
        10
    }
    async fn key(&self) -> Result<String> {
        Ok(format!("{}{}", self.protocol7().await, self.addr().await?))
    }
    async fn is_tls(&self) -> bool {
        false
    }
}
