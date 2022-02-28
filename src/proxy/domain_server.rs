use super::domain::DomainListen;
use super::domain_stream;
use crate::config::config_toml;
use crate::stream::server;
use crate::stream::server::Server;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::broadcast;

pub struct DomainServer {
    executor: async_executors::TokioCt,
    group_version: i32,
    listen_shutdown_tx: broadcast::Sender<()>,
    shutdown_thread_tx: broadcast::Sender<bool>,
    common_config: config_toml::CommonConfig,
    listen_server: Rc<Box<dyn Server>>,
    key: String,
    domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
    tunnel_servers: tunnel_server::Server,
    tunnel2_servers: tunnel2_server::Server,
}

impl DomainServer {
    pub fn new(
        executor: async_executors::TokioCt,
        group_version: i32,
        listen_shutdown_tx: broadcast::Sender<()>,
        shutdown_thread_tx: broadcast::Sender<bool>,
        common_config: config_toml::CommonConfig,
        listen_server: Rc<Box<dyn Server>>,
        key: String,
        domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
        tunnel_servers: tunnel_server::Server,
        tunnel2_servers: tunnel2_server::Server,
    ) -> anyhow::Result<DomainServer> {
        Ok(DomainServer {
            executor,
            group_version,
            listen_shutdown_tx,
            shutdown_thread_tx,
            common_config,
            listen_server,
            key,
            domain_config_listen_map,
            tunnel_servers,
            tunnel2_servers,
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let shutdown_timeout = self.common_config.shutdown_timeout;
        let listen_server = self.listen_server.clone();
        let executor = self.executor.clone();
        let group_version = self.group_version;
        let key = self.key.clone();
        let domain_config_listen_map = self.domain_config_listen_map.clone();

        let proto = listen_server.protocol7().to_protocol4();
        let listen_addr = listen_server.listen_addr()?;
        let (tunnel2_listen, tunnel2_publish) =
            self.tunnel2_servers.listen(proto, &listen_addr).await;
        let (tunnel_listen, tunnel_publish) = self.tunnel_servers.listen().await;
        server::listen(
            tunnel_listen,
            tunnel2_listen,
            shutdown_timeout,
            listen_server,
            executor.clone(),
            self.listen_shutdown_tx.clone(),
            self.shutdown_thread_tx.clone(),
            move |protocol_name, stream, local_addr, remote_addr, ssl_domain, shutdown_tx| {
                let key = key.clone();
                let domain_config_listen_map = domain_config_listen_map.clone();
                let tunnel_publish = tunnel_publish.clone();
                let tunnel2_publish = tunnel2_publish.clone();

                let executor = executor.clone();
                async move {
                    let domain_config_listen = {
                        let domain_config_listen_map_borrow = domain_config_listen_map.borrow_mut();
                        let domain_listen = domain_config_listen_map_borrow.get(&key);
                        if domain_listen.is_none() {
                            log::error!(
                                "err:domain_config_listen_map => key:{} invalid, group_version:{}",
                                key,
                                group_version
                            );
                            return Ok(());
                        }
                        let domain_listen = domain_listen.unwrap();
                        let domain_config_listen = domain_listen.domain_config_listen.clone();
                        domain_config_listen
                    };

                    let mut domain_stream = domain_stream::DomainStream::new(
                        tunnel_publish,
                        tunnel2_publish,
                        executor,
                        group_version,
                        local_addr,
                        remote_addr,
                        ssl_domain,
                        shutdown_tx,
                        domain_config_listen,
                    )?;
                    domain_stream.start(protocol_name, stream).await?;
                    Ok(())
                }
            },
        )
        .await?;
        Ok(())
    }
}
