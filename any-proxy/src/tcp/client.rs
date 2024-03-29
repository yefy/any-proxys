use super::util;
use crate::config::config_toml::TcpConfig as Config;
use crate::stream::client;
use crate::tcp::stream::Stream;
use crate::Protocol7;
use any_base::stream_flow::{StreamFlow, StreamFlowErr, StreamFlowInfo};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;

pub struct ClientContext {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Arc<Config>,
}

pub struct Client {
    context: Arc<ClientContext>,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Arc<Config>,
    ) -> Result<Client> {
        Ok(Client {
            context: Arc::new(ClientContext {
                addr,
                timeout,
                config,
            }),
        })
    }
}

#[async_trait]
impl client::Client for Client {
    async fn connect(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<Box<dyn client::Connection + Send>> {
        let stream_err: StreamFlowErr = StreamFlowErr::Init;
        if info.is_some() {
            info.as_ref().unwrap().get_mut().err = stream_err;
        }
        Ok(Box::new(Connection::new(self.context.clone())?))
    }
}

pub struct Connection {
    context: Arc<ClientContext>,
}

impl Connection {
    pub fn new(context: Arc<ClientContext>) -> Result<Connection> {
        Ok(Connection { context })
    }
}

#[async_trait]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<(Protocol7, StreamFlow, SocketAddr, SocketAddr)> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.context.timeout, TcpStream::connect(&self.context.addr))
                .await
            {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        stream_err = StreamFlowErr::WriteTimeout;
                        Err(anyhow!(
                            "err:client.stream timeout => addr:{}",
                            self.context.addr
                        ))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream reset => addr:{}",
                            self.context.addr
                        ))
                    }
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream => addr:{}, kind:{:?}, e:{}",
                            self.context.addr,
                            e.kind(),
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.stream timeout => addr:{}",
                        self.context.addr
                    ))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if info.is_some() {
                    info.as_ref().unwrap().get_mut().err = stream_err;
                }
                Err(e)
            }
            Ok(tcp_stream) => {
                util::set_stream(&tcp_stream, &self.context.config);
                let local_addr = tcp_stream
                    .local_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.local_addr => e:{}", e))?;
                let remote_addr = tcp_stream
                    .peer_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.peer_addr => e:{}", e))?;
                let stream = Stream::new(tcp_stream, self.context.config.clone());
                let stream = StreamFlow::new(stream, None);
                Ok((Protocol7::Tcp, stream, local_addr, remote_addr))
            }
        }
    }
}
