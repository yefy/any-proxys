use super::domain_config;
use super::domain_server;
use super::proxy;
use crate::config::config_toml;
use any_tunnel::client;
use any_tunnel::server;
use any_tunnel2::client as client2;
use any_tunnel2::server as server2;
use async_trait::async_trait;
use awaitgroup::WaitGroup;
use futures_util::task::LocalSpawnExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct DomainListen {
    pub domain_config_listen: Rc<domain_config::DomainConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Domain {
    executor: async_executors::TokioCt,
    group_version: i32,
    domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
    shutdown_thread_tx: broadcast::Sender<bool>,
    domain_server_wait_group: WaitGroup,
    tunnel_clients: client::Client,
    tunnel_servers: server::Server,
    tunnel2_clients: client2::Client,
    tunnel2_servers: server2::Server,
}

impl Domain {
    pub fn new(
        executor: async_executors::TokioCt,
        group_version: i32,
        tunnel_clients: client::Client,
        tunnel_servers: server::Server,
        tunnel2_clients: client2::Client,
        tunnel2_servers: server2::Server,
    ) -> anyhow::Result<Domain> {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        Ok(Domain {
            executor,
            group_version,
            domain_config_listen_map: Rc::new(RefCell::new(HashMap::new())),
            shutdown_thread_tx,
            domain_server_wait_group: WaitGroup::new(),
            tunnel_clients,
            tunnel_servers,
            tunnel2_clients,
            tunnel2_servers,
        })
    }
}

#[async_trait(?Send)]
impl proxy::Proxy for Domain {
    async fn start(&mut self, config: &config_toml::ConfigToml) -> anyhow::Result<()> {
        let domain_config = domain_config::DomainConfig::new()?;
        if config._domain.is_none() {
            return Ok(());
        }

        let domain_config_listen_map = domain_config
            .parse_config(
                config,
                self.tunnel_clients.clone(),
                self.tunnel2_clients.clone(),
            )
            .await?;
        let keys = self
            .domain_config_listen_map
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys.iter() {
            if domain_config_listen_map.get(key).is_none() {
                let _ = self
                    .domain_config_listen_map
                    .borrow()
                    .get(key)
                    .unwrap()
                    .listen_shutdown_tx
                    .send(());
                self.domain_config_listen_map.borrow_mut().remove(key);
            }
        }

        let mut key_domain_config_listens = Vec::new();
        for (key, domain_config_listen) in domain_config_listen_map {
            let (domain_config_listen, listen_shutdown_tx) = {
                let domain_config_listen_map_borrow = self.domain_config_listen_map.borrow();
                let domain_listen = domain_config_listen_map_borrow.get(&key);
                if domain_listen.is_none() {
                    key_domain_config_listens.push((key, domain_config_listen));
                    continue;
                }

                let domain_listen = domain_listen.unwrap();
                let domain_config_listen = domain_config::DomainConfig::merger(
                    &domain_listen.domain_config_listen,
                    domain_config_listen,
                );
                if domain_config_listen.is_err() {
                    continue;
                }
                let domain_config_listen = domain_config_listen?;
                (
                    domain_config_listen,
                    domain_listen.listen_shutdown_tx.clone(),
                )
            };
            self.domain_config_listen_map.borrow_mut().insert(
                key,
                DomainListen {
                    domain_config_listen: Rc::new(domain_config_listen),
                    listen_shutdown_tx,
                },
            );
        }

        for (key, domain_config_listen) in key_domain_config_listens {
            let executor = self.executor.clone();
            let group_version = self.group_version;
            let (listen_shutdown_tx, _) = broadcast::channel(100);
            let shutdown_thread_tx = self.shutdown_thread_tx.clone();
            let common_config = domain_config_listen.common.clone();
            let listen_server = domain_config_listen.listen_server.clone();

            let domain_listen = DomainListen {
                domain_config_listen: Rc::new(domain_config_listen),
                listen_shutdown_tx: listen_shutdown_tx.clone(),
            };
            self.domain_config_listen_map
                .borrow_mut()
                .insert(key.clone(), domain_listen);

            let domain_config_listen_map = self.domain_config_listen_map.clone();
            let tunnel_servers = self.tunnel_servers.clone();
            let tunnel2_servers = self.tunnel2_servers.clone();

            let worker = self.domain_server_wait_group.worker().add();
            self.executor
                .spawn_local(async move {
                    scopeguard::defer! {
                        worker.done();
                    }
                    let ret: anyhow::Result<()> = async {
                        let domain_server = domain_server::DomainServer::new(
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
                        )?;
                        domain_server.start().await?;
                        Ok(())
                    }
                    .await;
                    ret.unwrap_or_else(|e| log::error!("err:Domain.start => e:{}", e));
                })
                .unwrap_or_else(|e| log::error!("{}", e));
        }

        Ok(())
    }
    async fn stop(&self, is_fast_shutdown: bool) -> anyhow::Result<()> {
        let _ = self.shutdown_thread_tx.send(is_fast_shutdown)?;
        Ok(())
    }
    async fn wait(&self) -> anyhow::Result<()> {
        self.domain_server_wait_group.wait().await;
        Ok(())
    }
}
