use super::port_config;
use super::port_server;
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
pub struct PortListen {
    pub port_config_listen: Rc<port_config::PortConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Port {
    executor: async_executors::TokioCt,
    group_version: i32,
    port_config_listen_map: Rc<RefCell<HashMap<String, PortListen>>>,
    shutdown_thread_tx: broadcast::Sender<bool>,
    port_server_wait_group: WaitGroup,
    tunnel_clients: client::Client,
    tunnel_servers: server::Server,
    tunnel2_clients: client2::Client,
    tunnel2_servers: server2::Server,
}

impl Port {
    pub fn new(
        executor: async_executors::TokioCt,
        group_version: i32,
        tunnel_clients: client::Client,
        tunnel_servers: server::Server,
        tunnel2_clients: client2::Client,
        tunnel2_servers: server2::Server,
    ) -> anyhow::Result<Port> {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        Ok(Port {
            executor,
            group_version,
            port_config_listen_map: Rc::new(RefCell::new(HashMap::new())),
            shutdown_thread_tx,
            port_server_wait_group: WaitGroup::new(),
            tunnel_clients,
            tunnel_servers,
            tunnel2_clients,
            tunnel2_servers,
        })
    }
}

#[async_trait(?Send)]
impl proxy::Proxy for Port {
    async fn start(&mut self, config: &config_toml::ConfigToml) -> anyhow::Result<()> {
        let port_config = port_config::PortConfig::new()?;
        if config._port.is_none() {
            return Ok(());
        }

        let port_config_listen_map = port_config
            .parse_config(
                config,
                self.tunnel_clients.clone(),
                self.tunnel2_clients.clone(),
            )
            .await?;
        let keys = self
            .port_config_listen_map
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys.iter() {
            if port_config_listen_map.get(key).is_none() {
                let _ = self
                    .port_config_listen_map
                    .borrow()
                    .get(key)
                    .unwrap()
                    .listen_shutdown_tx
                    .send(());
                self.port_config_listen_map.borrow_mut().remove(key);
            }
        }

        let mut key_port_config_listens = Vec::new();
        for (key, port_config_listen) in port_config_listen_map {
            let (port_config_listen, listen_shutdown_tx) = {
                let port_config_listen_map_borrow = self.port_config_listen_map.borrow();
                let port_listen = port_config_listen_map_borrow.get(&key);
                if port_listen.is_none() {
                    key_port_config_listens.push((key, port_config_listen));
                    continue;
                }

                let port_listen = port_listen.unwrap();
                let port_config_listen = port_config::PortConfig::merger(
                    &port_listen.port_config_listen,
                    port_config_listen,
                );
                if port_config_listen.is_err() {
                    continue;
                }
                let port_config_listen = port_config_listen?;
                (port_config_listen, port_listen.listen_shutdown_tx.clone())
            };
            self.port_config_listen_map.borrow_mut().insert(
                key,
                PortListen {
                    port_config_listen: Rc::new(port_config_listen),
                    listen_shutdown_tx,
                },
            );
        }

        for (key, port_config_listen) in key_port_config_listens {
            let executor = self.executor.clone();
            let group_version = self.group_version;
            let (listen_shutdown_tx, _) = broadcast::channel(100);
            let shutdown_thread_tx = self.shutdown_thread_tx.clone();
            let common_config = port_config_listen.port_config_context.common.clone();
            let listen_server = port_config_listen.listen_server.clone();

            let port_listen = PortListen {
                port_config_listen: Rc::new(port_config_listen),
                listen_shutdown_tx: listen_shutdown_tx.clone(),
            };
            self.port_config_listen_map
                .borrow_mut()
                .insert(key.clone(), port_listen);

            let port_config_listen_map = self.port_config_listen_map.clone();
            let tunnel_servers = self.tunnel_servers.clone();
            let tunnel2_servers = self.tunnel2_servers.clone();

            let worker = self.port_server_wait_group.worker().add();
            self.executor
                .spawn_local(async move {
                    scopeguard::defer! {
                        worker.done();
                    }
                    let ret: anyhow::Result<()> = async {
                        let port_server = port_server::PortServer::new(
                            executor,
                            group_version,
                            listen_shutdown_tx,
                            shutdown_thread_tx,
                            common_config,
                            listen_server,
                            key,
                            port_config_listen_map,
                            tunnel_servers,
                            tunnel2_servers,
                        )?;
                        port_server.start().await?;
                        Ok(())
                    }
                    .await;
                    ret.unwrap_or_else(|e| log::error!("err:Port.start => e:{}", e));
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
        self.port_server_wait_group.wait().await;
        Ok(())
    }
}
