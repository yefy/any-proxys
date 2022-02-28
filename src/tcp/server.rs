use super::util as tcp_util;
use crate::config::config_toml::TcpConfig as Config;
use crate::stream::server;
use crate::stream::stream_flow;
use crate::util;
use crate::Protocol7;
use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

pub struct Server {
    addr: SocketAddr,
    reuseport: bool,
    config: Config,
}

impl Server {
    pub fn new(addr: SocketAddr, reuseport: bool, config: Config) -> anyhow::Result<Server> {
        Ok(Server {
            addr,
            reuseport,
            config,
        })
    }
}

#[async_trait(?Send)]
impl server::Server for Server {
    async fn listen(&self) -> anyhow::Result<Box<dyn server::Listener>> {
        let std_listener = tcp_util::bind(&self.addr, self.reuseport)?;
        let listener = TcpListener::from_std(std_listener)?;
        Ok(Box::new(Listener::new(listener, self.config.clone())?))
    }
    fn listen_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.addr.clone())
    }
    fn sni(&self) -> Option<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>> {
        None
    }
    fn set_sni(&self, _sni: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>) {}
    fn protocol7(&self) -> Protocol7 {
        Protocol7::Tcp
    }
}

pub struct Listener {
    listener: TcpListener,
    config: Config,
}

impl Listener {
    pub fn new(listener: TcpListener, config: Config) -> anyhow::Result<Listener> {
        Ok(Listener { listener, config })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> anyhow::Result<(Box<dyn server::Connection>, bool)> {
        let ret: anyhow::Result<(TcpStream, SocketAddr)> = async {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "err:Listener.accept => kind:{:?}, e:{}",
                        e.kind(),
                        e
                    ));
                }
            }
        }
        .await;
        let (stream, remote_addr) = ret?;
        tcp_util::set_stream(&stream, &self.config);
        Ok((Box::new(Connection::new(stream, remote_addr)?), false))
    }
}

pub struct Connection {
    stream: Option<TcpStream>,
    remote_addr: Option<SocketAddr>,
}

impl Connection {
    pub fn new(stream: TcpStream, remote_addr: SocketAddr) -> anyhow::Result<Connection> {
        Ok(Connection {
            stream: Some(stream),
            remote_addr: Some(remote_addr),
        })
    }
}

#[async_trait(?Send)]
impl server::Connection for Connection {
    async fn stream(
        &mut self,
    ) -> anyhow::Result<
        Option<(
            Protocol7,
            stream_flow::StreamFlow,
            SocketAddr,
            Option<String>,
        )>,
    > {
        if self.stream.is_none() {
            return Ok(None);
        }
        let tcp_stream = self.stream.take().unwrap();
        let remote_addr = self.remote_addr.take().unwrap();
        let stream = stream_flow::StreamFlow::new(Box::new(tcp_stream));
        Ok(Some((Protocol7::Tcp, stream, remote_addr, None)))
    }
}
