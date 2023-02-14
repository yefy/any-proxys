use super::stream_flow::StreamFlow;
use super::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;

#[async_trait]
pub trait PeerStreamConnect: Send + Sync {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)>;
    fn connect_addr(&self) -> Result<SocketAddr>; //这个是域名获取到的ip地址
    fn address(&self) -> String; //地址或域名  配上文件上的
    fn protocol4(&self) -> Protocol4; //tcp 或 udp  四层协议
    fn protocol7(&self) -> String; //tcp udp  quic http 等7层协议
    fn max_stream_size(&self) -> usize;
    fn min_stream_cache_size(&self) -> usize;
    fn channel_size(&self) -> usize;
    fn stream_send_timeout(&self) -> usize;
    fn stream_recv_timeout(&self) -> usize;
    fn key(&self) -> Result<String>;
}

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
        let stream = TcpStream::connect(self.connect_addr()?)
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
    fn connect_addr(&self) -> Result<SocketAddr> {
        let connect_addr = self
            .addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "err:empty address"))
            .map_err(|e| anyhow!("err:to_socket_addrs => e:{}", e))?;
        Ok(connect_addr)
    }
    fn address(&self) -> String {
        self.addr.clone()
    }
    fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }
    fn protocol7(&self) -> String {
        "tcp".to_string()
    }
    fn max_stream_size(&self) -> usize {
        self.max_stream_size
    }
    fn min_stream_cache_size(&self) -> usize {
        self.min_stream_cache_size
    }
    fn channel_size(&self) -> usize {
        self.channel_size
    }
    fn stream_send_timeout(&self) -> usize {
        10
    }
    fn stream_recv_timeout(&self) -> usize {
        10
    }
    fn key(&self) -> Result<String> {
        Ok(format!("{}{}", self.protocol7(), self.connect_addr()?))
    }
}
