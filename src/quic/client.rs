use crate::stream::client;
use crate::stream::stream_any;
use crate::stream::stream_flow;
use crate::Protocol7;
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
    ) -> anyhow::Result<Client> {
        Ok(Client {
            addr,
            timeout,
            endpoint,
            ssl_domain: ssl_domain.clone(),
        })
    }
}

#[async_trait(?Send)]
impl client::Client for Client {
    async fn connect(
        &self,
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> anyhow::Result<Box<dyn client::Connection>> {
        let local_addr = self.endpoint.local_addr()?;
        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        let ret: anyhow::Result<quinn::Connection> = async {
            let connect = self
                .endpoint
                .connect(self.addr.clone(), self.ssl_domain.as_str())?;
            match tokio::time::timeout(self.timeout, connect).await {
                Ok(ret) => match ret {
                    Err(e) => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow::anyhow!(
                            "err:client.connect =>  addr:{}, ssl_domain:{}, e:{}",
                            self.addr,
                            self.ssl_domain,
                            e
                        ))
                    }
                    Ok(new_connection) => {
                        let quinn::NewConnection { connection, .. } = new_connection;
                        Ok(connection)
                    }
                },
                Err(_) => {
                    stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                    Err(anyhow::anyhow!("err:client.connect timeout"))
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
    ) -> anyhow::Result<Connection> {
        Ok(Connection {
            local_addr,
            addr,
            timeout,
            ssl_domain: ssl_domain.clone(),
            connection,
        })
    }
}

#[async_trait(?Send)]
impl client::Connection for Connection {
    async fn stream(
        &self,
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> anyhow::Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)> {
        let local_addr = self.local_addr.clone();
        let remote_addr = self.connection.remote_address();
        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        let ret: anyhow::Result<(quinn::SendStream, quinn::RecvStream)> = async {
            match tokio::time::timeout(self.timeout, self.connection.open_bi()).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(e) => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow::anyhow!(
                            "err:client.stream =>  addr:{}, ssl_domain:{}, e:{}",
                            self.addr,
                            self.ssl_domain,
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
            Ok(quic_stream) => {
                let stream = stream_flow::StreamFlow::new(Box::new(stream_any::StreamAny::Quic(
                    quic_stream,
                )));
                Ok((Protocol7::Quic, stream, local_addr, remote_addr))
            }
        }
    }
}
