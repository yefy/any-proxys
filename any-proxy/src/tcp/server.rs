use super::util as tcp_util;
use crate::config::config_toml::TcpConfig as Config;
use crate::stream::server;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use crate::util;
use crate::Protocol7;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

pub struct Server {
    addr: SocketAddr,
    reuseport: bool,
    config: Arc<Config>,
}

impl Server {
    pub fn new(addr: SocketAddr, reuseport: bool, config: Config) -> Result<Server> {
        Ok(Server {
            addr,
            reuseport,
            config: Arc::new(config),
        })
    }
}

#[async_trait(?Send)]
impl server::Server for Server {
    fn stream_send_timeout(&self) -> usize {
        return self.config.tcp_send_timeout;
    }
    fn stream_recv_timeout(&self) -> usize {
        return self.config.tcp_recv_timeout;
    }

    async fn listen(&self) -> Result<Box<dyn server::Listener>> {
        let std_listener = tcp_util::bind(&self.addr, self.reuseport)
            .map_err(|e| anyhow!("err:tcp_util::bind => e:{}", e))?;
        let listener = TcpListener::from_std(std_listener)
            .map_err(|e| anyhow!("err:TcpListener::from_std => e:{}", e))?;
        Ok(Box::new(
            Listener::new(listener, self.config.clone())
                .map_err(|e| anyhow!("err:Listener::new => e:{}", e))?,
        ))
    }
    fn listen_addr(&self) -> Result<SocketAddr> {
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
    config: Arc<Config>,
}

impl Listener {
    pub fn new(listener: TcpListener, config: Arc<Config>) -> Result<Listener> {
        Ok(Listener { listener, config })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        let ret: Result<(TcpStream, SocketAddr)> = async {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    return Err(anyhow!(
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
        Ok((
            Box::new(
                Connection::new(stream, remote_addr, self.config.clone())
                    .map_err(|e| anyhow!("err:Connection::new => e:{}", e))?,
            ),
            false,
        ))
    }
}

pub struct Connection {
    stream: Option<TcpStream>,
    remote_addr: Option<SocketAddr>,
    config: Arc<Config>,
}

impl Connection {
    pub fn new(
        stream: TcpStream,
        remote_addr: SocketAddr,
        config: Arc<Config>,
    ) -> Result<Connection> {
        Ok(Connection {
            stream: Some(stream),
            remote_addr: Some(remote_addr),
            config,
        })
    }
}

#[async_trait(?Send)]
impl server::Connection for Connection {
    async fn stream(&mut self) -> Result<Option<(stream_flow::StreamFlow, ServerStreamInfo)>> {
        if self.stream.is_none() {
            return Ok(None);
        }
        let tcp_stream = self.stream.take().unwrap();
        let remote_addr = self.remote_addr.take().unwrap();
        let local_addr = tcp_stream.local_addr().unwrap();
        #[cfg(unix)]
        let fd = tcp_stream.as_raw_fd();
        #[cfg(not(unix))]
        let fd = 0;

        let mut stream = stream_flow::StreamFlow::new(fd, Box::new(tcp_stream));
        let read_timeout = tokio::time::Duration::from_secs(self.config.tcp_recv_timeout as u64);
        let write_timeout = tokio::time::Duration::from_secs(self.config.tcp_send_timeout as u64);
        stream.set_config(read_timeout, write_timeout, None);
        Ok(Some((
            stream,
            ServerStreamInfo {
                protocol7: Protocol7::Tcp,
                remote_addr: remote_addr,
                local_addr: Some(local_addr),
                domain: None,
            },
        )))
    }
}
