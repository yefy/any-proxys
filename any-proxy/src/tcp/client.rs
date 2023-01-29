use super::util;
use crate::config::config_toml::TcpConfig as Config;
use crate::stream::client;
use crate::stream::stream_flow;
use crate::Protocol7;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use tokio::net::TcpStream;

pub struct Client {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Config,
}

impl Client {
    pub fn new(addr: SocketAddr, timeout: tokio::time::Duration, config: Config) -> Result<Client> {
        Ok(Client {
            addr,
            timeout,
            config,
        })
    }
}

#[async_trait(?Send)]
impl client::Client for Client {
    async fn connect(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<Box<dyn client::Connection>> {
        let stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if info.is_some() {
            info.as_mut().unwrap().err = stream_err;
        }
        Ok(Box::new(Connection::new(
            self.addr.clone(),
            self.timeout.clone(),
            self.config.clone(),
        )?))
    }
}

pub struct Connection {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Config,
}

impl Connection {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Config,
    ) -> Result<Connection> {
        Ok(Connection {
            addr,
            timeout,
            config,
        })
    }
}

#[async_trait(?Send)]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)> {
        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(&self.addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                        Err(anyhow!("err:client.stream timeout"))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow!("err:client.stream reset"))
                    }
                    Err(e) => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow!("err:client.stream => kind:{:?}, e:{}", e.kind(), e))
                    }
                },
                Err(_) => {
                    stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                    Err(anyhow!("err:client.stream timeout"))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if info.is_some() {
                    info.as_mut().unwrap().err = stream_err;
                }
                Err(e)
            }
            Ok(tcp_stream) => {
                util::set_stream(&tcp_stream, &self.config);
                let local_addr = tcp_stream
                    .local_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.local_addr => e:{}", e))?;
                let remote_addr = tcp_stream
                    .peer_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.peer_addr => e:{}", e))?;
                #[cfg(unix)]
                let fd = tcp_stream.as_raw_fd();
                #[cfg(not(unix))]
                let fd = 0;

                let stream = stream_flow::StreamFlow::new(fd, Box::new(tcp_stream));
                Ok((Protocol7::Tcp, stream, local_addr, remote_addr))
            }
        }
    }
}
