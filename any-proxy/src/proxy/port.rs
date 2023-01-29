use super::port_config;
use super::port_server;
use super::proxy;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::upstream::upstream;
use crate::TunnelClients;
use crate::Tunnels;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::executor_local_spawn::ExecutorsLocal;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct PortListen {
    pub port_config_listen: Rc<port_config::PortConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Port {
    executor_local_spawn: ExecutorLocalSpawn,
    tunnels: Tunnels,
    port_config_listen_map: Rc<RefCell<HashMap<String, PortListen>>>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
}

impl Port {
    pub fn new(
        executors: ExecutorsLocal,
        tunnels: Tunnels,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<Port> {
        let executor_local_spawn = ExecutorLocalSpawn::new(executors);
        let tmp_file_id = Rc::new(RefCell::new(0 as u64));

        Ok(Port {
            executor_local_spawn,
            tunnels,
            port_config_listen_map: Rc::new(RefCell::new(HashMap::new())),
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
        })
    }
}

#[async_trait(?Send)]
impl proxy::Proxy for Port {
    async fn start(
        &mut self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
    ) -> Result<()> {
        let port_config = port_config::PortConfig::new()
            .map_err(|e| anyhow!("err:PortConfig::new => e:{}", e))?;
        if config._port.is_none() {
            return Ok(());
        }
        let tunnel_clients = TunnelClients {
            client: self.tunnels.client.clone(),
            client2: self.tunnels.client2.clone(),
        };
        let port_config_listen_map = port_config
            .parse_config(
                config,
                ups,
                tunnel_clients,
                #[cfg(feature = "anyproxy-ebpf")]
                Some(self.ebpf_add_sock_hash.clone()),
            )
            .await
            .map_err(|e| anyhow!("err:port_config.parse_config => e:{}", e))?;
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
            let (listen_shutdown_tx, _) = broadcast::channel(100);
            let common_config = port_config_listen
                .port_config_context
                .stream_config_context
                .common
                .clone();
            let listen_server = port_config_listen.listen_server.clone();

            let port_listen = PortListen {
                port_config_listen: Rc::new(port_config_listen),
                listen_shutdown_tx: listen_shutdown_tx.clone(),
            };
            self.port_config_listen_map
                .borrow_mut()
                .insert(key.clone(), port_listen);

            let port_config_listen_map = self.port_config_listen_map.clone();
            let tunnels = self.tunnels.clone();
            let tmp_file_id = self.tmp_file_id.clone();

            #[cfg(feature = "anyproxy-ebpf")]
            let ebpf_add_sock_hash = self.ebpf_add_sock_hash.clone();
            let session_id = self.session_id.clone();
            self.executor_local_spawn
                ._start(move |executors| async move {
                    let port_server = port_server::PortServer::new(
                        executors,
                        tunnels,
                        listen_shutdown_tx,
                        common_config,
                        listen_server,
                        key,
                        port_config_listen_map,
                        tmp_file_id,
                        #[cfg(feature = "anyproxy-ebpf")]
                        ebpf_add_sock_hash,
                        session_id,
                    )
                    .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    port_server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                    Ok(())
                });
        }

        Ok(())
    }
    async fn stop(&self, is_fast_shutdown: bool) -> Result<()> {
        self.executor_local_spawn.stop(is_fast_shutdown, 10).await;
        Ok(())
    }
    async fn send(&self, is_fast_shutdown: bool) -> Result<()> {
        self.executor_local_spawn.send(is_fast_shutdown);
        Ok(())
    }
}
