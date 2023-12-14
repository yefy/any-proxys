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
use any_tunnel2::client as tunnel_client;
use any_tunnel2::peer_stream_connect::PeerStreamConnect;
use any_tunnel2::stream_flow::StreamFlow;
use any_tunnel2::Protocol4;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct PeerStreamConnectTcp {
    host: ArcString,
    address: SocketAddr, //ip:port, domain:port
    tcp_config: TcpConfig,
    tcp_connect: Arc<tcp_connect::Connect>,
}

impl PeerStreamConnectTcp {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        tcp_config: TcpConfig,
    ) -> PeerStreamConnectTcp {
        let host = ArcString::new(host);
        let tcp_connect =
            tcp_connect::Connect::new(host.clone(), address.clone(), tcp_config.clone());
        let tcp_connect = Arc::new(tcp_connect);
        PeerStreamConnectTcp {
            host,
            address,
            tcp_config,
            tcp_connect,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let (stream, connect_info) = self
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
        // let connect_addr = util::lookup_host(self.timeout, &self.address)
        //     .await
        //     .map_err(|e| anyhow!("err:util::lookup_host => e:{}", e))?;
        // Ok(connect_addr)
        Ok(self.address.clone())
    }

    async fn host(&self) -> ArcString {
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
    async fn is_tls(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct PeerStreamConnectSsl {
    host: ArcString,
    address: SocketAddr, //ip:port, domain:port
    tcp_config: TcpConfig,
    ssl_connect: ArcMutexTokio<ssl_connect::Connect>,
}

impl PeerStreamConnectSsl {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        tcp_config: TcpConfig,
    ) -> PeerStreamConnectSsl {
        let host = ArcString::new(host);
        let ssl_connect = ssl_connect::Connect::new(
            host.clone(),
            address.clone(),
            ssl_domain,
            tcp_config.clone(),
        );
        let ssl_connect = ArcMutexTokio::new(ssl_connect);
        PeerStreamConnectSsl {
            host,
            address,
            tcp_config,
            ssl_connect,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectSsl {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let (stream, connect_info) = self
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
        // let connect_addr = util::lookup_host(self.timeout, &self.address)
        //     .await
        //     .map_err(|e| anyhow!("err:util::lookup_host => e:{}", e))?;
        // Ok(connect_addr)
        Ok(self.address.clone())
    }

    async fn host(&self) -> ArcString {
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
    async fn is_tls(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct PeerStreamConnectQuic {
    host: ArcString,
    address: SocketAddr, //ip:port, domain:port
    //ssl_domain: String,
    //endpoints: Arc<endpoints::Endpoints>,
    quic_config: QuicConfig,
    quic_connect: ArcMutexTokio<quic_connect::Connect>,
}

impl PeerStreamConnectQuic {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        endpoints: Arc<endpoints::Endpoints>,
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
        let quic_connect = ArcMutexTokio::new(quic_connect);
        PeerStreamConnectQuic {
            host,
            address,
            //ssl_domain,
            //endpoints,
            quic_config,
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
                    Ok((w, r)) => {
                        let r = any_base::io::buf_reader::BufReader::new(r);
                        let w = any_base::io::buf_writer::BufWriter::new(w);
                        let stream = StreamFlow::new(0, Box::new(r), Box::new(w));
                        Ok((stream, local_addr, remote_addr))
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
        Ok(self.address.clone())
    }

    async fn host(&self) -> ArcString {
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
        _request_id: Option<ArcString>,
        stream_info: ArcMutex<StreamFlowInfo>,
        _run_time: Option<Arc<Box<dyn Runtime>>>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)> {
        let start_time = Instant::now();
        let connect = self.client.connect(self.connect.clone()).await;

        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if connect.is_err() {
            stream_err = stream_flow::StreamFlowErr::WriteErr;
        }
        if stream_info.is_some() {
            stream_info.get_mut().err = stream_err;
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
                peer_stream_size: None,
                max_stream_size: None,
                min_stream_cache_size: None,
                channel_size: None,
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
        self.connect.host().await
    }
}
