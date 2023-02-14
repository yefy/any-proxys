use super::util as quic_util;
use crate::config::config_toml::QuicConfig as Config;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::stream::server;
use crate::stream::server::ServerStreamInfo;
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
        Ok(Box::new(Listener::new(
            endpoint,
            self.config.clone(),
            self.addr.clone(),
        )?))
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
    addr: SocketAddr,
}

impl Listener {
    pub fn new(
        endpoint: quinn::Endpoint,
        config: Arc<Config>,
        addr: SocketAddr,
    ) -> Result<Listener> {
        Ok(Listener {
            endpoint,
            config,
            addr,
        })
    }
}

impl Listener {
    async fn do_accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        let new_conn = self.endpoint.accept().await;
        if new_conn.is_none() {
            return Err(anyhow!("quic accept nil addr:{}", self.addr));
        }
        let new_conn = new_conn.unwrap();

        let remote_addr = new_conn.remote_address();
        let new_conn = new_conn.await.map_err(|e| {
            anyhow!(
                "err:get bi_streams => remote_addr:{}, local_addr:{}, e:{}",
                remote_addr,
                self.addr,
                e
            )
        })?;

        let remote_addr = new_conn.remote_address();

        let handshake_data = new_conn.handshake_data().ok_or(anyhow!(
            "err:new_conn.handshake_data => remote_addr:{}, local_addr:{}",
            remote_addr,
            self.addr,
        ))?;

        let domain = match handshake_data.downcast_ref::<quinn::crypto::rustls::HandshakeData>() {
            Some(handshake_data) => handshake_data.server_name.clone().ok_or(anyhow!(
                "err:server_name nil => remote_addr:{}, local_addr:{}",
                remote_addr,
                self.addr,
            ))?,
            None => {
                return Err(anyhow!(
                    "err:server_name nil => remote_addr:{}, local_addr:{}",
                    remote_addr,
                    self.addr,
                ))
            }
        };

        return Ok((
            Box::new(Connection::new(
                new_conn,
                remote_addr,
                domain,
                self.config.clone(),
                self.addr.clone(),
            )?),
            true,
        ));
    }
}

#[async_trait(?Send)]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        loop {
            let ret = self.do_accept().await;
            if let Err(e) = ret {
                log::error!("e:quic accept => e:{}", e);
                continue;
            }
            return ret;
        }
    }
}

pub struct Connection {
    conn: quinn::Connection,
    remote_addr: SocketAddr,
    domain: String,
    config: Arc<Config>,
    listen_addr: SocketAddr,
}

impl Connection {
    pub fn new(
        conn: quinn::Connection,
        remote_addr: SocketAddr,
        domain: String,
        config: Arc<Config>,
        listen_addr: SocketAddr,
    ) -> Result<Connection> {
        Ok(Connection {
            conn,
            remote_addr,
            domain,
            config,
            listen_addr,
        })
    }
}

#[async_trait(?Send)]
impl server::Connection for Connection {
    async fn stream(&mut self) -> Result<Option<(stream_flow::StreamFlow, ServerStreamInfo)>> {
        let stream = self.conn.accept_bi().await;
        match stream {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                // log::error!(
                //     "bi_streams.next ApplicationClosed listen_addr:{}, remote_addr:{}",
                //     self.listen_addr,
                //     self.remote_addr
                // );
                return Ok(None);
            }
            Err(e) => {
                // log::error!(
                //     "err:bi_streams connection => listen_addr:{}, remote_addr:{}, e:{:?}",
                //     self.listen_addr,
                //     self.remote_addr,
                //     e,
                // );
                return Err(anyhow!(
                    "err:bi_streams connection => listen_addr:{}, remote_addr:{}, e:{}",
                    self.listen_addr,
                    self.remote_addr,
                    e
                ));
            }
            Ok((w, r)) => {
                let mut stream = stream_flow::StreamFlow::new(0, Box::new(r), Box::new(w));
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
