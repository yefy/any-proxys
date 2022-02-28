use super::util;
use crate::config::config_toml::TcpConfig as Config;
use crate::stream::client;
use crate::stream::stream_flow;
use crate::Protocol7;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpStream;

pub struct Client {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Config,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Config,
    ) -> anyhow::Result<Client> {
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
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> anyhow::Result<Box<dyn client::Connection>> {
        let stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if stream_info.is_some() {
            stream_info.as_mut().unwrap().err = stream_err;
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
    ) -> anyhow::Result<Connection> {
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
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> anyhow::Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)> {
        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        let ret: anyhow::Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(&self.addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                        Err(anyhow::anyhow!("err:client.stream timeout"))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow::anyhow!("err:client.stream reset"))
                    }
                    Err(e) => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow::anyhow!(
                            "err:client.stream => kind:{:?}, e:{}",
                            e.kind(),
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                    Err(anyhow::anyhow!("err:client.stream timeout"))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if stream_info.is_some() {
                    stream_info.as_mut().unwrap().err = stream_err;
                }
                Err(e)
            }
            Ok(tcp_stream) => {
                util::set_stream(&tcp_stream, &self.config);
                let local_addr = tcp_stream.local_addr()?;
                let remote_addr = tcp_stream.peer_addr()?;
                let stream = stream_flow::StreamFlow::new(Box::new(tcp_stream));
                Ok((Protocol7::Tcp, stream, local_addr, remote_addr))
            }
        }
    }
}
