use crate::quic::stream::Stream;
use crate::stream::client;
use crate::Protocol7;
use any_base::stream_flow::{StreamFlow, StreamFlowErr, StreamFlowInfo};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct ClientContext {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    ssl_domain: Arc<String>,
}

pub struct Client {
    context: Arc<ClientContext>,
    endpoint: quinn::Endpoint,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        endpoint: quinn::Endpoint,
        ssl_domain: Arc<String>,
    ) -> Result<Client> {
        Ok(Client {
            context: Arc::new(ClientContext {
                addr,
                timeout,
                ssl_domain,
            }),
            endpoint,
        })
    }
}

#[async_trait]
impl client::Client for Client {
    async fn connect(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<Box<dyn client::Connection + Send>> {
        let local_addr = self
            .endpoint
            .local_addr()
            .map_err(|e| anyhow!("err:endpoint.local_addr => e:{}", e))?;
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<quinn::Connection> = async {
            let connect = self
                .endpoint
                .connect(self.context.addr.clone(), self.context.ssl_domain.as_str())
                .map_err(|e| {
                    anyhow!(
                        "err:endpoint.connect => addr:{}, e:{}",
                        self.context.addr,
                        e
                    )
                })?;
            match tokio::time::timeout(self.context.timeout, connect).await {
                Ok(ret) => match ret {
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.connect =>  addr:{}, ssl_domain:{}, e:{}",
                            self.context.addr,
                            self.context.ssl_domain,
                            e
                        ))
                    }
                    Ok(connection) => Ok(connection),
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.connect timeout =>  addr:{}, ssl_domain:{}",
                        self.context.addr,
                        self.context.ssl_domain,
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
            Ok(connection) => Ok(Box::new(Connection {
                context: self.context.clone(),
                local_addr,
                connection,
            })),
        }
    }
}

pub struct Connection {
    context: Arc<ClientContext>,
    local_addr: SocketAddr,
    connection: quinn::Connection,
}

impl Connection {
    pub fn new(
        context: Arc<ClientContext>,
        local_addr: SocketAddr,
        connection: quinn::Connection,
    ) -> Result<Connection> {
        Ok(Connection {
            context,
            local_addr,
            connection,
        })
    }
}

#[async_trait]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<(Protocol7, StreamFlow, SocketAddr, SocketAddr)> {
        let local_addr = self.local_addr.clone();
        let remote_addr = self.connection.remote_address();
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<(quinn::SendStream, quinn::RecvStream)> = async {
            match tokio::time::timeout(self.context.timeout, self.connection.open_bi()).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream =>  addr:{}, ssl_domain:{}, e:{}",
                            self.context.addr,
                            self.context.ssl_domain,
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.stream timeout => addr:{}, ssl_domain:{}",
                        self.context.addr,
                        self.context.ssl_domain
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
            Ok((w, r)) => {
                let stream = Stream::new(r, w);
                let stream = StreamFlow::new(stream, None);
                Ok((Protocol7::Quic, stream, local_addr, remote_addr))
            }
        }
    }
}
