use super::util as quic_util;
use crate::config::config_toml::QuicConfig as Config;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::stream::server;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_any;
use crate::stream::stream_flow;
use crate::util;
use crate::Protocol7;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Server {
    config: Arc<Config>,
    reuseport: bool,
    addr: SocketAddr,
    sni_rustls_map: RefCell<Arc<util::rustls::ResolvesServerCertUsingSNI>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Option<Arc<any_ebpf::AddSockHash>>,
}

impl Server {
    pub fn new(
        addr: SocketAddr,
        reuseport: bool,
        config: Config,
        sni_rustls_map: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Option<Arc<any_ebpf::AddSockHash>>,
    ) -> Result<Server> {
        Ok(Server {
            config: std::sync::Arc::new(config),
            reuseport,
            addr,
            sni_rustls_map: RefCell::new(sni_rustls_map),
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
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
        let endpoint = quic_util::listen(
            &self.config,
            self.reuseport,
            &self.addr,
            self.sni_rustls_map.borrow().clone(),
            #[cfg(feature = "anyproxy-ebpf")]
            &self.ebpf_add_sock_hash,
        )
        .await
        .map_err(|e| anyhow!("err:quic_util::listen => e:{}", e))?;
        Ok(Box::new(Listener::new(endpoint, self.config.clone())?))
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
    endpoint: quinn::Endpoint,
    config: Arc<Config>,
}

impl Listener {
    pub fn new(endpoint: quinn::Endpoint, config: Arc<Config>) -> Result<Listener> {
        Ok(Listener { endpoint, config })
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        loop {
            let new_conn = self.endpoint.accept().await;
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

            let conn = new_conn
                .await
                .map_err(|e| anyhow!("err:get bi_streams => e:{}", e))?;
            return Ok((
                Box::new(Connection::new(
                    conn,
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
    conn: quinn::Connection,
    remote_addr: SocketAddr,
    domain: String,
    config: Arc<Config>,
}

impl Connection {
    pub fn new(
        conn: quinn::Connection,
        remote_addr: SocketAddr,
        domain: String,
        config: Arc<Config>,
    ) -> Result<Connection> {
        Ok(Connection {
            conn,
            remote_addr,
            domain,
            config,
        })
    }
}

#[async_trait(?Send)]
impl server::Connection for Connection {
    async fn stream(&mut self) -> Result<Option<(stream_flow::StreamFlow, ServerStreamInfo)>> {
        let stream = self.conn.accept_bi().await;
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
                stream.set_config(read_timeout, write_timeout, None);
                return Ok(Some((
                    stream,
                    ServerStreamInfo {
                        protocol7: Protocol7::Quic,
                        remote_addr: self.remote_addr.clone(),
                        local_addr: None,
                        domain: Some(self.domain.clone()),
                    },
                )));
            }
        }
    }
}
