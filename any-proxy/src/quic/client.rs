use crate::quic::stream::Stream;
use crate::stream::client;
use crate::Protocol7;
use any_base::stream_flow::{StreamFlow, StreamFlowErr, StreamFlowInfo};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

pub struct Client {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    endpoint: quinn::Endpoint,
    ssl_domain: String,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        endpoint: quinn::Endpoint,
        ssl_domain: &String,
    ) -> Result<Client> {
        Ok(Client {
            addr,
            timeout,
            endpoint,
            ssl_domain: ssl_domain.clone(),
        })
    }
}

#[async_trait]
impl client::Client for Client {
    async fn connect(
        &self,
        info: ArcMutex<StreamFlowInfo>,
    ) -> Result<Box<dyn client::Connection + Send>> {
        let local_addr = self
            .endpoint
            .local_addr()
            .map_err(|e| anyhow!("err:endpoint.local_addr => e:{}", e))?;
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<quinn::Connection> = async {
            let connect = self
                .endpoint
                .connect(self.addr.clone(), self.ssl_domain.as_str())
                .map_err(|e| anyhow!("err:endpoint.connect => addr:{}, e:{}", self.addr, e))?;
            match tokio::time::timeout(self.timeout, connect).await {
                Ok(ret) => match ret {
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.connect =>  addr:{}, ssl_domain:{}, e:{}",
                            self.addr,
                            self.ssl_domain,
                            e
                        ))
                    }
                    Ok(connection) => Ok(connection),
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.connect timeout =>  addr:{}, ssl_domain:{}",
                        self.addr,
                        self.ssl_domain,
                    ))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if info.is_some() {
                    info.get_mut().err = stream_err;
                }
                Err(e)
            }
            Ok(connection) => Ok(Box::new(Connection {
                local_addr,
                addr: self.addr.clone(),
                timeout: self.timeout.clone(),
                ssl_domain: self.ssl_domain.clone(),
                connection,
            })),
        }
    }
}

pub struct Connection {
    local_addr: SocketAddr,
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    ssl_domain: String,
    connection: quinn::Connection,
}

impl Connection {
    pub fn new(
        local_addr: SocketAddr,
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        ssl_domain: &String,
        connection: quinn::Connection,
    ) -> Result<Connection> {
        Ok(Connection {
            local_addr,
            addr,
            timeout,
            ssl_domain: ssl_domain.clone(),
            connection,
        })
    }
}

#[async_trait]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: ArcMutex<StreamFlowInfo>,
    ) -> Result<(Protocol7, StreamFlow, SocketAddr, SocketAddr)> {
        let local_addr = self.local_addr.clone();
        let remote_addr = self.connection.remote_address();
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<(quinn::SendStream, quinn::RecvStream)> = async {
            match tokio::time::timeout(self.timeout, self.connection.open_bi()).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream =>  addr:{}, ssl_domain:{}, e:{}",
                            self.addr,
                            self.ssl_domain,
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.stream timeout => addr:{}, ssl_domain:{}",
                        self.addr,
                        self.ssl_domain
                    ))
                }
            }
        }
        .await;
        match ret {
            Err(e) => {
                if info.is_some() {
                    info.get_mut().err = stream_err;
                }
                Err(e)
            }
            Ok((w, r)) => {
                let stream = Stream::new(r, w);
                let stream = StreamFlow::new(0, stream);
                Ok((Protocol7::Quic, stream, local_addr, remote_addr))
            }
        }
    }
}
