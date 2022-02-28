use crate::stream::server;
use crate::stream::stream_flow;
use crate::Protocol7;
use any_tunnel::server as tunnel_server;
use any_tunnel::stream as tunnel_stream;
use async_trait::async_trait;
use std::net::SocketAddr;

pub struct Listener {
    protocol7: Protocol7,
    listener: tunnel_server::Listener,
}

impl Listener {
    pub fn new(
        protocol7: Protocol7,
        listener: tunnel_server::Listener,
    ) -> anyhow::Result<Listener> {
        Ok(Listener {
            protocol7,
            listener,
        })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> anyhow::Result<(Box<dyn server::Connection>, bool)> {
        let ret: anyhow::Result<(tunnel_stream::Stream, SocketAddr, SocketAddr)> = async {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    return Err(anyhow::anyhow!("err:Listener.accept => e:{}", e));
                }
            }
        }
        .await;
        let (stream, _, remote_addr) = ret?;
        Ok((
            Box::new(Connection::new(
                self.protocol7.clone(),
                stream,
                remote_addr,
            )?),
            false,
        ))
    }
}

pub struct Connection {
    protocol7: Protocol7,
    stream: Option<tunnel_stream::Stream>,
    remote_addr: Option<SocketAddr>,
}

impl Connection {
    pub fn new(
        protocol7: Protocol7,
        stream: tunnel_stream::Stream,
        remote_addr: SocketAddr,
    ) -> anyhow::Result<Connection> {
        Ok(Connection {
            protocol7,
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
        let stream = self.stream.take().unwrap();
        let remote_addr = self.remote_addr.take().unwrap();
        let stream = stream_flow::StreamFlow::new(Box::new(stream));
        Ok(Some((self.protocol7.clone(), stream, remote_addr, None)))
    }
}
