use crate::stream::server;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use crate::Protocol7;
use any_base::typ::ArcMutex;
use any_tunnel2::server as tunnel_server;
use any_tunnel2::stream as tunnel_stream;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

pub struct Listener {
    protocol7: Protocol7,
    listener: tunnel_server::Listener,
    stream_send_timeout: usize,
    stream_recv_timeout: usize,
    is_tls: bool,
}

impl Listener {
    pub fn new(
        protocol7: Protocol7,
        listener: tunnel_server::Listener,
        stream_send_timeout: usize,
        stream_recv_timeout: usize,
        is_tls: bool,
    ) -> Result<Listener> {
        Ok(Listener {
            protocol7,
            listener,
            stream_send_timeout,
            stream_recv_timeout,
            is_tls,
        })
    }
}

#[async_trait]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        let ret: Result<(tunnel_stream::Stream, SocketAddr, SocketAddr)> = async {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    return Err(anyhow!("err:Listener.accept => e:{}", e));
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
                self.stream_send_timeout,
                self.stream_recv_timeout,
                self.is_tls,
            )?),
            false,
        ))
    }
}

pub struct Connection {
    protocol7: Protocol7,
    stream: Option<tunnel_stream::Stream>,
    remote_addr: Option<SocketAddr>,
    stream_send_timeout: usize,
    stream_recv_timeout: usize,
    is_tls: bool,
}

impl Connection {
    pub fn new(
        protocol7: Protocol7,
        stream: tunnel_stream::Stream,
        remote_addr: SocketAddr,
        stream_send_timeout: usize,
        stream_recv_timeout: usize,
        is_tls: bool,
    ) -> Result<Connection> {
        Ok(Connection {
            protocol7,
            stream: Some(stream),
            remote_addr: Some(remote_addr),
            stream_send_timeout,
            stream_recv_timeout,
            is_tls,
        })
    }
}

#[async_trait]
impl server::Connection for Connection {
    async fn stream(&mut self) -> Result<Option<(stream_flow::StreamFlow, ServerStreamInfo)>> {
        if self.stream.is_none() {
            return Ok(None);
        }
        let stream = self.stream.take().unwrap();
        let remote_addr = self.remote_addr.take().unwrap();
        let (r, w) = any_base::io::split::split(stream);
        let mut stream =
            stream_flow::StreamFlow::new(0, ArcMutex::new(Box::new(r)), ArcMutex::new(Box::new(w)));
        let read_timeout = tokio::time::Duration::from_secs(self.stream_recv_timeout as u64);
        let write_timeout = tokio::time::Duration::from_secs(self.stream_send_timeout as u64);
        stream.set_config(read_timeout, write_timeout, ArcMutex::default());

        Ok(Some((
            stream,
            ServerStreamInfo {
                protocol7: self.protocol7.clone(),
                remote_addr: remote_addr,
                local_addr: None,
                domain: None,
                is_tls: self.is_tls,
            },
        )))
    }
}
