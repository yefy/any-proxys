use crate::config::config_toml::QuicConfig;
use crate::config::config_toml::TcpConfig;
use crate::quic::connect as quic_connect;
use crate::quic::endpoints;
use crate::ssl::connect as ssl_connect;
use crate::stream::connect;
use crate::stream::connect::Connect as stream_connect;
use crate::stream::connect::ConnectInfo;
use crate::stream::stream_flow;
use crate::tcp::connect as tcp_connect;
use crate::Protocol7;
use any_base::executor_local_spawn::Runtime;
use any_base::stream_flow::StreamFlowInfo;
use any_base::typ::{ArcMutex, ArcMutexTokio};
use any_base::util::ArcString;
use any_tunnel::client as tunnel_client;
use any_tunnel::peer_stream_connect::PeerStreamConnect;
use any_tunnel::stream_flow::StreamFlow;
use any_tunnel::Protocol4;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Instant;

pub struct PeerStreamConnectTcpContext {
    pub host: ArcString,
    pub address: SocketAddr, //ip:port, domain:port
    pub tcp_config: TcpConfig,
    pub max_stream_size: usize,
    pub min_stream_cache_size: usize,
    pub channel_size: usize,
    pub tcp_connect: tcp_connect::Connect,
}

#[derive(Clone)]
pub struct PeerStreamConnectTcp {
    context: Arc<PeerStreamConnectTcpContext>,
}

impl PeerStreamConnectTcp {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        tcp_config: TcpConfig,
        max_stream_size: usize,
        min_stream_cache_size: usize,
        channel_size: usize,
    ) -> PeerStreamConnectTcp {
        let host = ArcString::new(host);
        let tcp_connect =
            tcp_connect::Connect::new(host.clone(), address.clone(), tcp_config.clone());
        PeerStreamConnectTcp {
            context: Arc::new(PeerStreamConnectTcpContext {
                host,
                address,
                tcp_config,
                max_stream_size,
                min_stream_cache_size,
                channel_size,
                tcp_connect,
            }),
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let (stream, connect_info) = self
            .context
            .tcp_connect
            .connect(None, ArcMutex::default(), None)
            .await?;
        //let (read_timeout, write_timeout, info, r, w, fd) = stream.split_stream();
        //let r = any_base::io::buf_reader::BufReader::new(r);
        //let w = any_base::io::buf_writer::BufWriter::new(w);
        //let mut stream = StreamFlow::new(fd, Box::new(r), Box::new(w));
        //stream.set_config(read_timeout, write_timeout, info);
        return Ok((
            stream,
            connect_info.local_addr.clone(),
            connect_info.remote_addr.clone(),
        ));
        /*
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(connect_addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => Err(anyhow!(
                        "err:client.stream timeout => connect_addr:{}",
                        connect_addr
                    )),
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        Err(anyhow!("err:client.stream reset"))
                    }
                    Err(e) => Err(anyhow!(
                        "err:client.stream => connect_addr:{}, kind:{:?}, e:{}",
                        connect_addr,
                        e.kind(),
                        e
                    )),
                },
                Err(_) => Err(anyhow!(
                    "err:client.stream timeout => connect_addr:{}",
                    connect_addr
                )),
            }
        }
        .await;

        match ret {
            Err(e) => Err(e),
            Ok(tcp_stream) => {
                tcp_util::set_stream(&tcp_stream, &self.tcp_config);
                let local_addr = tcp_stream.local_addr()?;
                let remote_addr = tcp_stream.peer_addr()?;
                let (r, w) = tokio::io::split(tcp_stream);
                let r = any_base::io::buf_reader::BufReader::new(r);
                let w = any_base::io::buf_writer::BufWriter::new(w);
                Ok((
                    StreamFlow::new(0, Box::new(r), Box::new(w)),
                    local_addr,
                    remote_addr,
                ))
            }
        }

         */
    }
    async fn addr(&self) -> Result<SocketAddr> {
        Ok(self.context.address.clone())
    }

    async fn host(&self) -> ArcString {
        self.context.host.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }

    async fn protocol7(&self) -> String {
        Protocol7::TunnelTcp.to_string()
    }
    async fn max_stream_size(&self) -> usize {
        self.context.max_stream_size
    }
    async fn min_stream_cache_size(&self) -> usize {
        self.context.min_stream_cache_size
    }
    async fn channel_size(&self) -> usize {
        self.context.channel_size
    }
    async fn stream_send_timeout(&self) -> usize {
        self.context.tcp_config.tcp_send_timeout
    }
    async fn stream_recv_timeout(&self) -> usize {
        self.context.tcp_config.tcp_recv_timeout
    }
    async fn key(&self) -> Result<String> {
        Ok(format!(
            "{}{}{}",
            self.protocol7().await,
            self.addr().await?,
            self.host().await
        ))
    }
    async fn is_tls(&self) -> bool {
        false
    }
}

pub struct PeerStreamConnectSslContext {
    pub host: ArcString,
    pub address: SocketAddr, //ip:port, domain:port
    pub ssl_domain: String,
    pub tcp_config: TcpConfig,
    pub max_stream_size: usize,
    pub min_stream_cache_size: usize,
    pub channel_size: usize,
    pub ssl_connect: ArcMutexTokio<ssl_connect::Connect>,
}

#[derive(Clone)]
pub struct PeerStreamConnectSsl {
    context: Arc<PeerStreamConnectSslContext>,
}

impl PeerStreamConnectSsl {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        tcp_config: TcpConfig,
        max_stream_size: usize,
        min_stream_cache_size: usize,
        channel_size: usize,
    ) -> PeerStreamConnectSsl {
        let host = ArcString::new(host);
        let ssl_connect = ssl_connect::Connect::new(
            host.clone(),
            address.clone(),
            ssl_domain.clone(),
            tcp_config.clone(),
        );
        PeerStreamConnectSsl {
            context: Arc::new(PeerStreamConnectSslContext {
                host,
                address,
                ssl_domain,
                tcp_config,
                max_stream_size,
                min_stream_cache_size,
                channel_size,
                ssl_connect: ArcMutexTokio::new(ssl_connect),
            }),
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectSsl {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let (stream, connect_info) = self
            .context
            .ssl_connect
            .get()
            .await
            .connect(None, ArcMutex::default(), None)
            .await?;
        //let (read_timeout, write_timeout, info, r, w, fd) = stream.split_stream();
        //let r = any_base::io::buf_reader::BufReader::new(r);
        //let w = any_base::io::buf_writer::BufWriter::new(w);
        //let mut stream = StreamFlow::new(fd, Box::new(r), Box::new(w));
        //stream.set_config(read_timeout, write_timeout, info);
        return Ok((
            stream,
            connect_info.local_addr.clone(),
            connect_info.remote_addr.clone(),
        ));
        /*
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(connect_addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => Err(anyhow!(
                        "err:client.stream timeout => connect_addr:{}",
                        connect_addr
                    )),
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        Err(anyhow!("err:client.stream reset"))
                    }
                    Err(e) => Err(anyhow!(
                        "err:client.stream => connect_addr:{}, kind:{:?}, e:{}",
                        connect_addr,
                        e.kind(),
                        e
                    )),
                },
                Err(_) => Err(anyhow!(
                    "err:client.stream timeout => connect_addr:{}",
                    connect_addr
                )),
            }
        }
        .await;

        match ret {
            Err(e) => Err(e),
            Ok(tcp_stream) => {
                tcp_util::set_stream(&tcp_stream, &self.tcp_config);
                let local_addr = tcp_stream.local_addr()?;
                let remote_addr = tcp_stream.peer_addr()?;
                let (r, w) = tokio::io::split(tcp_stream);
                let r = any_base::io::buf_reader::BufReader::new(r);
                let w = any_base::io::buf_writer::BufWriter::new(w);
                Ok((
                    StreamFlow::new(0, Box::new(r), Box::new(w)),
                    local_addr,
                    remote_addr,
                ))
            }
        }

         */
    }
    async fn addr(&self) -> Result<SocketAddr> {
        Ok(self.context.address.clone())
    }

    async fn host(&self) -> ArcString {
        self.context.host.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }

    async fn protocol7(&self) -> String {
        Protocol7::TunnelSsl.to_string()
    }
    async fn max_stream_size(&self) -> usize {
        self.context.max_stream_size
    }
    async fn min_stream_cache_size(&self) -> usize {
        self.context.min_stream_cache_size
    }
    async fn channel_size(&self) -> usize {
        self.context.channel_size
    }
    async fn stream_send_timeout(&self) -> usize {
        self.context.tcp_config.tcp_send_timeout
    }
    async fn stream_recv_timeout(&self) -> usize {
        self.context.tcp_config.tcp_recv_timeout
    }
    async fn key(&self) -> Result<String> {
        Ok(format!(
            "{}{}{}",
            self.protocol7().await,
            self.addr().await?,
            self.host().await
        ))
    }

    async fn is_tls(&self) -> bool {
        true
    }
}

pub struct PeerStreamConnectQuicContext {
    pub host: ArcString,
    pub address: SocketAddr, //ip:port, domain:port
    pub ssl_domain: String,
    pub endpoints: Arc<endpoints::Endpoints>,
    pub max_stream_size: usize,
    pub min_stream_cache_size: usize,
    pub channel_size: usize,
    pub quic_config: QuicConfig,
}

#[derive(Clone)]
pub struct PeerStreamConnectQuic {
    context: Arc<PeerStreamConnectQuicContext>,
    quic_connect: ArcMutexTokio<Box<dyn connect::Connect>>,
}

impl PeerStreamConnectQuic {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        endpoints: Arc<endpoints::Endpoints>,
        max_stream_size: usize,
        min_stream_cache_size: usize,
        channel_size: usize,
        quic_config: QuicConfig,
    ) -> PeerStreamConnectQuic {
        let host = ArcString::new(host);
        let quic_connect = quic_connect::Connect::new(
            host.clone(),
            address.clone(),
            ssl_domain.clone(),
            endpoints.clone(),
            quic_config.clone(),
        );
        let quic_connect: Box<dyn connect::Connect> = Box::new(quic_connect);
        let quic_connect = ArcMutexTokio::new(quic_connect);
        PeerStreamConnectQuic {
            context: Arc::new(PeerStreamConnectQuicContext {
                host,
                address,
                ssl_domain,
                endpoints,
                max_stream_size,
                min_stream_cache_size,
                channel_size,
                quic_config,
            }),
            quic_connect,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectQuic {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let (stream, connect_info) = self
            .quic_connect
            .get()
            .await
            .connect(None, ArcMutex::default(), None)
            .await?;
        //let (read_timeout, write_timeout, info, r, w, fd) = stream.split_stream();
        //let r = any_base::io::buf_reader::BufReader::new(r);
        //let w = any_base::io::buf_writer::BufWriter::new(w);
        //let mut stream = StreamFlow::new(fd, Box::new(r), Box::new(w));
        //stream.set_config(read_timeout, write_timeout, info);
        return Ok((
            stream,
            connect_info.local_addr.clone(),
            connect_info.remote_addr.clone(),
        ));

        /*
        let endpoint = self.endpoints.endpoint()?;
        let local_addr = endpoint.local_addr()?;
        let connect = endpoint
            .connect(connect_addr.clone(), self.ssl_domain.as_str())
            .map_err(|e| anyhow!("err:endpoint.connect => e:{}", e))?;

        let ret: Result<quinn::Connection> = async {
            match tokio::time::timeout(self.timeout, connect).await {
                Ok(ret) => match ret {
                    Ok(connection) => Ok(connection),
                    Err(e) => Err(anyhow!(
                        "err:client.stream => connect_addr:{}, e:{}",
                        connect_addr,
                        e
                    )),
                },
                Err(_) => Err(anyhow!(
                    "err:client.stream timeout => connect_addr:{}",
                    connect_addr
                )),
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
                                "err:client.stream =>  connect_addr:{}, ssl_domain:{}, e:{}",
                                connect_addr,
                                self.ssl_domain,
                                e
                            )),
                        },
                        Err(_) => Err(anyhow!(
                            "err:client.stream timeout => connect_addr:{}",
                            connect_addr
                        )),
                    }
                }
                .await;

                match ret {
                    Err(e) => Err(e),
                    Ok((w, r)) => {
                        let r = any_base::io::buf_reader::BufReader::new(r);
                        let w = any_base::io::buf_writer::BufWriter::new(w);
                        Ok((
                            StreamFlow::new(0, Box::new(r), Box::new(w)),
                            local_addr,
                            remote_addr,
                        ))
                    }
                }
            }
        }

         */
    }
    async fn addr(&self) -> Result<SocketAddr> {
        // let connect_addr = util::lookup_host(self.timeout, &self.address)
        //     .await
        //     .map_err(|e| anyhow!("err:util::lookup_host => e:{}", e))?;
        // Ok(connect_addr)
        Ok(self.context.address.clone())
    }

    async fn host(&self) -> ArcString {
        self.context.host.clone()
    }

    async fn protocol4(&self) -> Protocol4 {
        Protocol4::UDP
    }

    async fn protocol7(&self) -> String {
        Protocol7::TunnelQuic.to_string()
    }
    async fn max_stream_size(&self) -> usize {
        self.context.max_stream_size
    }
    async fn min_stream_cache_size(&self) -> usize {
        self.context.min_stream_cache_size
    }
    async fn channel_size(&self) -> usize {
        self.context.channel_size
    }
    async fn stream_send_timeout(&self) -> usize {
        self.context.quic_config.quic_send_timeout
    }
    async fn stream_recv_timeout(&self) -> usize {
        self.context.quic_config.quic_recv_timeout
    }
    async fn key(&self) -> Result<String> {
        Ok(format!(
            "{}{}{}",
            self.protocol7().await,
            self.addr().await?,
            self.host().await
        ))
    }

    async fn is_tls(&self) -> bool {
        true
    }
}

pub struct Connect {
    client: tunnel_client::Client,
    pub connect: Arc<Box<dyn PeerStreamConnect>>,
}

impl Connect {
    pub fn new(client: tunnel_client::Client, connect: Box<dyn PeerStreamConnect>) -> Connect {
        Connect {
            client,
            connect: Arc::new(connect),
        }
    }
}

#[async_trait]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        request_id: Option<ArcString>,
        info: ArcMutex<StreamFlowInfo>,
        run_time: Option<Arc<Box<dyn Runtime>>>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)> {
        let start_time = Instant::now();
        let peer_stream_size = Some(Arc::new(AtomicUsize::new(0)));
        let connect = self
            .client
            .connect(
                request_id,
                self.connect.clone(),
                peer_stream_size.clone(),
                run_time.unwrap(),
            )
            .await;

        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if connect.is_err() {
            stream_err = stream_flow::StreamFlowErr::WriteErr;
        }
        if info.is_some() {
            info.get_mut().err = stream_err;
        }

        let (stream, local_addr, remote_addr) = connect?;
        let elapsed = start_time.elapsed().as_secs_f32();

        let mut stream = stream_flow::StreamFlow::new(0, stream);

        let read_timeout =
            tokio::time::Duration::from_secs(self.connect.stream_recv_timeout().await as u64);
        let write_timeout =
            tokio::time::Duration::from_secs(self.connect.stream_send_timeout().await as u64);
        stream.set_config(read_timeout, write_timeout, ArcMutex::default());
        Ok((
            stream,
            ConnectInfo {
                protocol7: Protocol7::from_string(&self.connect.protocol7().await)?,
                domain: self.connect.host().await,
                elapsed,
                local_addr,
                remote_addr,
                peer_stream_size,
                max_stream_size: Some(self.connect.max_stream_size().await),
                min_stream_cache_size: Some(self.connect.min_stream_cache_size().await),
                channel_size: Some(self.connect.channel_size().await),
            },
        ))
    }

    async fn addr(&self) -> Result<SocketAddr> {
        self.connect.addr().await
    }

    async fn host(&self) -> Result<ArcString> {
        Ok(self.connect.host().await)
    }
    async fn is_tls(&self) -> bool {
        self.connect.is_tls().await
    }
    async fn protocol7(&self) -> String {
        self.connect.protocol7().await
    }
    async fn domain(&self) -> ArcString {
        self.connect.host().await.into()
    }
}
