use crate::config::config_toml::TcpConfig;
use crate::quic::endpoints;
use crate::stream::connect;
use crate::stream::stream_any::StreamAny;
use crate::stream::stream_flow;
use crate::tcp::util as tcp_util;
use crate::util::util;
use crate::Protocol7;
use any_tunnel2::client as tunnel_client;
use any_tunnel2::peer_stream_connect::PeerStreamConnect;
use any_tunnel2::stream_flow::StreamFlow;
use any_tunnel2::Protocol4;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;

#[derive(Clone)]
pub struct PeerStreamConnectTcp {
    address: String, //ip:port, domain:port
    timeout: tokio::time::Duration,
    tcp_config: TcpConfig,
}

impl PeerStreamConnectTcp {
    pub fn new(
        address: String, //ip:port, domain:port
        timeout: tokio::time::Duration,
        tcp_config: TcpConfig,
    ) -> PeerStreamConnectTcp {
        PeerStreamConnectTcp {
            address,
            timeout,
            tcp_config,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(
        &self,
        connect_addr: &SocketAddr,
    ) -> anyhow::Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let ret: anyhow::Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(connect_addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        Err(anyhow::anyhow!("err:client.stream timeout"))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        Err(anyhow::anyhow!("err:client.stream reset"))
                    }
                    Err(e) => Err(anyhow::anyhow!(
                        "err:client.stream => kind:{:?}, e:{}",
                        e.kind(),
                        e
                    )),
                },
                Err(_) => Err(anyhow::anyhow!("err:client.stream timeout")),
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
                    StreamFlow::new(Box::new(tcp_stream)),
                    local_addr,
                    remote_addr,
                ))
            }
        }
    }
    async fn connect_addr(&self) -> anyhow::Result<SocketAddr> {
        let connect_addr = util::lookup_host(self.timeout, &self.address).await?;
        Ok(connect_addr)
    }

    async fn address(&self) -> String {
        self.address.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }

    async fn protocol7(&self) -> String {
        Protocol7::Tunnel2Tcp.to_string()
    }
}

#[derive(Clone)]
pub struct PeerStreamConnectQuic {
    address: String, //ip:port, domain:port
    ssl_domain: String,
    timeout: tokio::time::Duration,
    endpoints: Arc<endpoints::Endpoints>,
}

impl PeerStreamConnectQuic {
    pub fn new(
        address: String, //ip:port, domain:port
        ssl_domain: String,
        timeout: tokio::time::Duration,
        endpoints: Arc<endpoints::Endpoints>,
    ) -> PeerStreamConnectQuic {
        PeerStreamConnectQuic {
            address,
            ssl_domain,
            timeout,
            endpoints,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectQuic {
    async fn connect(
        &self,
        connect_addr: &SocketAddr,
    ) -> anyhow::Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let endpoint = self.endpoints.endpoint();
        let local_addr = endpoint.local_addr()?;
        let connect = endpoint.connect(connect_addr.clone(), self.ssl_domain.as_str())?;
        let ret: anyhow::Result<quinn::Connection> = async {
            match tokio::time::timeout(self.timeout, connect).await {
                Ok(ret) => match ret {
                    Ok(new_connection) => {
                        let quinn::NewConnection { connection, .. } = new_connection;
                        Ok(connection)
                    }
                    Err(e) => Err(anyhow::anyhow!("err:client.stream => e:{}", e)),
                },
                Err(_) => Err(anyhow::anyhow!("err:client.stream timeout")),
            }
        }
        .await;

        match ret {
            Err(e) => Err(e),
            Ok(connection) => {
                let remote_addr = connection.remote_address();
                let ret: anyhow::Result<(quinn::SendStream, quinn::RecvStream)> = async {
                    match tokio::time::timeout(self.timeout, connection.open_bi()).await {
                        Ok(ret) => match ret {
                            Ok(stream) => Ok(stream),
                            Err(e) => Err(anyhow::anyhow!(
                                "err:client.stream =>  ssl_domain:{}, e:{}",
                                self.ssl_domain,
                                e
                            )),
                        },
                        Err(_) => Err(anyhow::anyhow!("err:client.stream timeout")),
                    }
                }
                .await;

                match ret {
                    Err(e) => Err(e),
                    Ok(quic_stream) => Ok((
                        StreamFlow::new(Box::new(StreamAny::Quic(quic_stream))),
                        local_addr,
                        remote_addr,
                    )),
                }
            }
        }
    }
    async fn connect_addr(&self) -> anyhow::Result<SocketAddr> {
        let connect_addr = util::lookup_host(self.timeout, &self.address).await?;
        Ok(connect_addr)
    }

    async fn address(&self) -> String {
        self.address.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::UDP
    }

    async fn protocol7(&self) -> String {
        Protocol7::Tunnel2Quic.to_string()
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
    ) -> anyhow::Result<Connect> {
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
    ) -> anyhow::Result<(
        Protocol7,
        stream_flow::StreamFlow,
        String,
        f32,
        SocketAddr,
        SocketAddr,
    )> {
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
        Ok((
            Protocol7::from_string(self.connect.protocol7().await)?,
            stream_flow::StreamFlow::new(Box::new(stream)),
            self.connect.address().await,
            elapsed,
            local_addr,
            remote_addr,
        ))
    }
}
