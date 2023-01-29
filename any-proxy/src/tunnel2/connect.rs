use crate::config::config_toml::QuicConfig;
use crate::config::config_toml::TcpConfig;
use crate::quic::endpoints;
use crate::stream::connect;
use crate::stream::connect::ConnectInfo;
use crate::stream::stream_any::StreamAny;
use crate::stream::stream_flow;
use crate::tcp::util as tcp_util;
use crate::Protocol7;
use any_tunnel2::client as tunnel_client;
use any_tunnel2::peer_stream_connect::PeerStreamConnect;
use any_tunnel2::stream_flow::StreamFlow;
use any_tunnel2::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;

#[derive(Clone)]
pub struct PeerStreamConnectTcp {
    host: String,
    address: SocketAddr, //ip:port, domain:port
    tcp_config: TcpConfig,
    timeout: tokio::time::Duration,
}

impl PeerStreamConnectTcp {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        tcp_config: TcpConfig,
    ) -> PeerStreamConnectTcp {
        let timeout = tokio::time::Duration::from_secs(tcp_config.tcp_connect_timeout as u64);
        PeerStreamConnectTcp {
            host,
            address,
            tcp_config,
            timeout,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(
        &self,
        connect_addr: &SocketAddr,
    ) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(connect_addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        Err(anyhow!("err:client.stream timeout"))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        Err(anyhow!("err:client.stream reset"))
                    }
                    Err(e) => Err(anyhow!("err:client.stream => kind:{:?}, e:{}", e.kind(), e)),
                },
                Err(_) => Err(anyhow!("err:client.stream timeout")),
            }
        }
        .await;

        match ret {
            Err(e) => Err(e),
            Ok(tcp_stream) => {
                tcp_util::set_stream(&tcp_stream, &self.tcp_config);
                let local_addr = tcp_stream.local_addr()?;
                let remote_addr = tcp_stream.peer_addr()?;
                Ok((
                    StreamFlow::new(0, Box::new(tcp_stream)),
                    local_addr,
                    remote_addr,
                ))
            }
        }
    }
    async fn connect_addr(&self) -> Result<SocketAddr> {
        // let connect_addr = util::lookup_host(self.timeout, &self.address)
        //     .await
        //     .map_err(|e| anyhow!("err:util::lookup_host => e:{}", e))?;
        // Ok(connect_addr)
        Ok(self.address.clone())
    }

    async fn address(&self) -> String {
        self.host.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }

    async fn protocol7(&self) -> String {
        Protocol7::Tunnel2Tcp.to_string()
    }
    async fn stream_send_timeout(&self) -> usize {
        self.tcp_config.tcp_send_timeout
    }
    async fn stream_recv_timeout(&self) -> usize {
        self.tcp_config.tcp_recv_timeout
    }
}

#[derive(Clone)]
pub struct PeerStreamConnectQuic {
    host: String,
    address: SocketAddr, //ip:port, domain:port
    ssl_domain: String,
    endpoints: Arc<endpoints::Endpoints>,
    quic_config: QuicConfig,
    timeout: tokio::time::Duration,
}

impl PeerStreamConnectQuic {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        endpoints: Arc<endpoints::Endpoints>,
        quic_config: QuicConfig,
    ) -> PeerStreamConnectQuic {
        let timeout = tokio::time::Duration::from_secs(quic_config.quic_connect_timeout as u64);
        PeerStreamConnectQuic {
            host,
            address,
            ssl_domain,
            endpoints,
            quic_config,
            timeout,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectQuic {
    async fn connect(
        &self,
        connect_addr: &SocketAddr,
    ) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let endpoint = self.endpoints.endpoint()?;
        let local_addr = endpoint.local_addr()?;
        let connect = endpoint
            .connect(connect_addr.clone(), self.ssl_domain.as_str())
            .map_err(|e| anyhow!("err:endpoint.connect => e:{}", e))?;
        let ret: Result<quinn::Connection> = async {
            match tokio::time::timeout(self.timeout, connect).await {
                Ok(ret) => match ret {
                    Ok(connection) => Ok(connection),
                    Err(e) => Err(anyhow!("err:client.stream => e:{}", e)),
                },
                Err(_) => Err(anyhow!("err:client.stream timeout")),
            }
        }
        .await;

        match ret {
            Err(e) => Err(e),
            Ok(connection) => {
                let remote_addr = connection.remote_address();
                let ret: Result<(quinn::SendStream, quinn::RecvStream)> = async {
                    match tokio::time::timeout(self.timeout, connection.open_bi()).await {
                        Ok(ret) => match ret {
                            Ok(stream) => Ok(stream),
                            Err(e) => Err(anyhow!(
                                "err:client.stream =>  ssl_domain:{}, e:{}",
                                self.ssl_domain,
                                e
                            )),
                        },
                        Err(_) => Err(anyhow!("err:client.stream timeout")),
                    }
                }
                .await;

                match ret {
                    Err(e) => Err(e),
                    Ok(quic_stream) => {
                        let stream = StreamFlow::new(0, Box::new(StreamAny::Quic(quic_stream)));
                        Ok((stream, local_addr, remote_addr))
                    }
                }
            }
        }
    }
    async fn connect_addr(&self) -> Result<SocketAddr> {
        // let connect_addr = util::lookup_host(self.timeout, &self.address)
        //     .await
        //     .map_err(|e| anyhow!("err:util::lookup_host => e:{}", e))?;
        // Ok(connect_addr)
        Ok(self.address.clone())
    }

    async fn address(&self) -> String {
        self.host.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::UDP
    }

    async fn protocol7(&self) -> String {
        Protocol7::Tunnel2Quic.to_string()
    }
    async fn stream_send_timeout(&self) -> usize {
        self.quic_config.quic_send_timeout
    }
    async fn stream_recv_timeout(&self) -> usize {
        self.quic_config.quic_recv_buffer_size
    }
}

pub struct Connect {
    client: tunnel_client::Client,
    pub connect: Arc<Box<dyn PeerStreamConnect>>,
}

impl Connect {
    pub fn new(
        client: tunnel_client::Client,
        connect: Box<dyn PeerStreamConnect>,
    ) -> Result<Connect> {
        Ok(Connect {
            client,
            connect: Arc::new(connect),
        })
    }
}

#[async_trait(?Send)]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)> {
        let start_time = Instant::now();
        let connect = self.client.connect(self.connect.clone()).await;

        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if connect.is_err() {
            stream_err = stream_flow::StreamFlowErr::WriteErr;
        }
        if stream_info.is_some() {
            stream_info.as_mut().unwrap().err = stream_err;
        }

        let (stream, local_addr, remote_addr) = connect?;
        let elapsed = start_time.elapsed().as_secs_f32();

        let mut stream = stream_flow::StreamFlow::new(0, Box::new(stream));
        let read_timeout =
            tokio::time::Duration::from_secs(self.connect.stream_recv_timeout().await as u64);
        let write_timeout =
            tokio::time::Duration::from_secs(self.connect.stream_send_timeout().await as u64);
        stream.set_config(read_timeout, write_timeout, None);

        Ok((
            stream,
            ConnectInfo {
                protocol7: Protocol7::from_string(self.connect.protocol7().await)?,
                domain: self.connect.address().await,
                elapsed,
                local_addr,
                remote_addr,
                peer_stream_size: None,
                max_stream_size: None,
                min_stream_cache_size: None,
                channel_size: None,
            },
        ))
    }
}
