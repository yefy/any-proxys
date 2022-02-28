use super::util as quic_util;
use crate::config::config_toml::QuicConfig as Config;
use crate::stream::server;
use crate::stream::stream_any;
use crate::stream::stream_flow;
use crate::util;
use crate::Protocol7;
use async_trait::async_trait;
use futures_util::stream::StreamExt;
use std::cell::RefCell;
use std::net::SocketAddr;

pub struct Server {
    config: Config,
    reuseport: bool,
    addr: SocketAddr,
    sni_rustls_map: RefCell<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>>,
}

impl Server {
    pub fn new(
        addr: SocketAddr,
        reuseport: bool,
        config: Config,
        sni_rustls_map: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>,
    ) -> anyhow::Result<Server> {
        Ok(Server {
            config,
            reuseport,
            addr,
            sni_rustls_map: RefCell::new(sni_rustls_map),
        })
    }
}

#[async_trait(?Send)]
impl server::Server for Server {
    async fn listen(&self) -> anyhow::Result<Box<dyn server::Listener>> {
        let (_, incoming) = quic_util::listen(
            &self.config,
            self.reuseport,
            &self.addr,
            self.sni_rustls_map.borrow().clone(),
        )
        .await?;
        Ok(Box::new(Listener::new(incoming)?))
    }

    fn listen_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.addr.clone())
    }
    fn sni(&self) -> Option<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>> {
        Some(self.sni_rustls_map.borrow().clone())
    }
    fn set_sni(&self, sni: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>) {
        *self.sni_rustls_map.borrow_mut() = sni;
    }

    fn protocol7(&self) -> Protocol7 {
        Protocol7::Quic
    }
}

pub struct Listener {
    incoming: quinn::Incoming,
}

impl Listener {
    pub fn new(incoming: quinn::Incoming) -> anyhow::Result<Listener> {
        Ok(Listener { incoming })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> anyhow::Result<(Box<dyn server::Connection>, bool)> {
        loop {
            let new_conn = self.incoming.next().await;
            if new_conn.is_none() {
                continue;
            }
            let mut new_conn = new_conn.unwrap();
            let remote_addr = new_conn.remote_address();

            let handshake_data = new_conn.handshake_data().await?;

            let domain = match handshake_data.downcast_ref::<quinn::crypto::rustls::HandshakeData>()
            {
                Some(handshake_data) => handshake_data
                    .server_name
                    .clone()
                    .ok_or(anyhow::anyhow!("err:server_name nil"))?,
                None => {
                    return Err(anyhow::anyhow!("err:server_name nil"));
                }
            };

            let quinn::NewConnection { bi_streams, .. } = new_conn
                .await
                .map_err(|e| anyhow::anyhow!("err:get bi_streams => e:{}", e))?;
            return Ok((
                Box::new(Connection::new(bi_streams, remote_addr, domain)?),
                true,
            ));
        }
    }
}

pub struct Connection {
    bi_streams: quinn::IncomingBiStreams,
    remote_addr: SocketAddr,
    domain: String,
}

impl Connection {
    pub fn new(
        bi_streams: quinn::IncomingBiStreams,
        remote_addr: SocketAddr,
        domain: String,
    ) -> anyhow::Result<Connection> {
        Ok(Connection {
            bi_streams,
            remote_addr,
            domain,
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
        let stream = self.bi_streams.next().await;
        if stream.is_none() {
            return Ok(None);
        }
        let stream = stream.unwrap();
        match stream {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                //log::warn!("bi_streams.next ApplicationClosed");
                return Ok(None);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("err:bi_streams connection => e:{}", e));
            }
            Ok(quic_stream) => {
                let stream = stream_flow::StreamFlow::new(Box::new(stream_any::StreamAny::Quic(
                    quic_stream,
                )));
                return Ok(Some((
                    Protocol7::Quic,
                    stream,
                    self.remote_addr.clone(),
                    Some(self.domain.clone()),
                )));
            }
        }
    }
}
