use super::util as quic_util;
use crate::config::config_toml::QuicConfig as Config;
use crate::stream::server;
use crate::stream::stream_any;
use crate::stream::stream_flow;
use crate::util;
use crate::Protocol7;
use anyhow::anyhow;
use anyhow::Result;
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
    ) -> Result<Server> {
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
    fn stream_send_timeout(&self) -> usize {
        return self.config.quic_send_timeout;
    }
    fn stream_recv_timeout(&self) -> usize {
        return self.config.quic_recv_timeout;
    }
    async fn listen(&self) -> Result<Box<dyn server::Listener>> {
        let (_, incoming) = quic_util::listen(
            &self.config,
            self.reuseport,
            &self.addr,
            self.sni_rustls_map.borrow().clone(),
        )
        .await
        .map_err(|e| anyhow!("err:quic_util::listen => e:{}", e))?;
        Ok(Box::new(Listener::new(incoming, self.config.clone())?))
    }

    fn listen_addr(&self) -> Result<SocketAddr> {
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
    config: Config,
}

impl Listener {
    pub fn new(incoming: quinn::Incoming, config: Config) -> Result<Listener> {
        Ok(Listener { incoming, config })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        loop {
            let new_conn = self.incoming.next().await;
            if new_conn.is_none() {
                continue;
            }
            let mut new_conn = new_conn.unwrap();
            let remote_addr = new_conn.remote_address();

            let handshake_data = new_conn
                .handshake_data()
                .await
                .map_err(|e| anyhow!("err:new_conn.handshake_data => e:{}", e))?;

            let domain = match handshake_data.downcast_ref::<quinn::crypto::rustls::HandshakeData>()
            {
                Some(handshake_data) => handshake_data
                    .server_name
                    .clone()
                    .ok_or(anyhow!("err:server_name nil"))?,
                None => {
                    return Err(anyhow!("err:server_name nil"));
                }
            };

            let quinn::NewConnection { bi_streams, .. } = new_conn
                .await
                .map_err(|e| anyhow!("err:get bi_streams => e:{}", e))?;
            return Ok((
                Box::new(Connection::new(
                    bi_streams,
                    remote_addr,
                    domain,
                    self.config.clone(),
                )?),
                true,
            ));
        }
    }
}

pub struct Connection {
    bi_streams: quinn::IncomingBiStreams,
    remote_addr: SocketAddr,
    domain: String,
    config: Config,
}

impl Connection {
    pub fn new(
        bi_streams: quinn::IncomingBiStreams,
        remote_addr: SocketAddr,
        domain: String,
        config: Config,
    ) -> Result<Connection> {
        Ok(Connection {
            bi_streams,
            remote_addr,
            domain,
            config,
        })
    }
}

#[async_trait(?Send)]
impl server::Connection for Connection {
    async fn stream(
        &mut self,
    ) -> Result<
        Option<(
            Protocol7,
            stream_flow::StreamFlow,
            SocketAddr,
            Option<SocketAddr>,
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
                return Err(anyhow!("err:bi_streams connection => e:{}", e));
            }
            Ok(quic_stream) => {
                let mut stream = stream_flow::StreamFlow::new(
                    0,
                    Box::new(stream_any::StreamAny::Quic(quic_stream)),
                );
                let read_timeout =
                    tokio::time::Duration::from_secs(self.config.quic_recv_timeout as u64);
                let write_timeout =
                    tokio::time::Duration::from_secs(self.config.quic_send_timeout as u64);
                stream.set_config(read_timeout, write_timeout, true, None);
                return Ok(Some((
                    Protocol7::Quic,
                    stream,
                    self.remote_addr.clone(),
                    None,
                    Some(self.domain.clone()),
                )));
            }
        }
    }
}
