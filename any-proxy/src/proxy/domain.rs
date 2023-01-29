use super::domain_config;
use super::domain_server;
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
pub struct DomainListen {
    pub domain_config_listen: Rc<domain_config::DomainConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Domain {
    executor_local_spawn: ExecutorLocalSpawn,
    tunnels: Tunnels,
    domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
}

impl Domain {
    pub fn new(
        executors: ExecutorsLocal,
        tunnels: Tunnels,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<Domain> {
        let executor_local_spawn = ExecutorLocalSpawn::new(executors);
        let tmp_file_id = Rc::new(RefCell::new(0 as u64));
        Ok(Domain {
            executor_local_spawn,
            tunnels,
            domain_config_listen_map: Rc::new(RefCell::new(HashMap::new())),
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
        })
    }
}

#[async_trait(?Send)]
impl proxy::Proxy for Domain {
    async fn start(
        &mut self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
    ) -> Result<()> {
        let domain_config = domain_config::DomainConfig::new()
            .map_err(|e| anyhow!("err:DomainConfig::new => e:{}", e))?;
        if config._domain.is_none() {
            return Ok(());
        }

        let tunnel_clients = TunnelClients {
            client: self.tunnels.client.clone(),
            client2: self.tunnels.client2.clone(),
        };
        let domain_config_listen_map = domain_config
            .parse_config(
                config,
                ups.clone(),
                tunnel_clients,
                #[cfg(feature = "anyproxy-ebpf")]
                Some(self.ebpf_add_sock_hash.clone()),
            )
            .await
            .map_err(|e| anyhow!("err:domain_config.parse_config => e:{}", e))?;
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
            let (listen_shutdown_tx, _) = broadcast::channel(100);
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
            let tunnels = self.tunnels.clone();

            let tmp_file_id = self.tmp_file_id.clone();
            #[cfg(feature = "anyproxy-ebpf")]
            let ebpf_add_sock_hash = self.ebpf_add_sock_hash.clone();
            let session_id = self.session_id.clone();

            self.executor_local_spawn
                ._start(move |executors| async move {
                    let domain_server = domain_server::DomainServer::new(
                        executors,
                        tunnels,
                        listen_shutdown_tx,
                        common_config,
                        listen_server,
                        key,
                        domain_config_listen_map,
                        tmp_file_id,
                        #[cfg(feature = "anyproxy-ebpf")]
                        ebpf_add_sock_hash,
                        session_id,
                    )
                    .map_err(|e| anyhow!("err:DomainServer::new => e:{}", e))?;
                    domain_server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:domain_server.start => e:{}", e))?;
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
