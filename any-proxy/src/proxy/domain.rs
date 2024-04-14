use super::domain_config;
use super::domain_server;
use crate::anyproxy::anyproxy_work::{
    AnyproxyWorkDataNew, AnyproxyWorkDataSend, AnyproxyWorkDataStop, AnyproxyWorkDataWait,
};
use crate::proxy::domain_context::DomainContext;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::typ;
use any_base::typ::{ArcUnsafeAny, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct DomainListen {
    pub domain_config_listen: Arc<domain_config::DomainConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Domain {
    executor: ExecutorLocalSpawn,
    domain_config_listen_map: ShareRw<HashMap<String, DomainListen>>,
    context: Arc<DomainContext>,
}

impl Domain {
    pub fn new(value: typ::ArcUnsafeAny) -> Result<Domain> {
        let value = value.get::<AnyproxyWorkDataNew>();
        let executor = ExecutorLocalSpawn::new(value.executors.clone());
        Ok(Domain {
            executor,
            domain_config_listen_map: ShareRw::new(HashMap::new()),
            context: Arc::new(DomainContext::new()),
        })
    }
}

#[async_trait]
impl module::Server for Domain {
    async fn start(&mut self, ms: Modules, _value: ArcUnsafeAny) -> Result<()> {
        log::trace!(target: "main", "domain start");
        use crate::config::domain_core;
        let domain_core_conf = domain_core::main_conf_mut(&ms).await;
        if domain_core_conf.domain_config_listen_map.is_empty() {
            return Ok(());
        }

        let keys = self
            .domain_config_listen_map
            .get()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys.iter() {
            if domain_core_conf.domain_config_listen_map.get(key).is_none() {
                let _ = self
                    .domain_config_listen_map
                    .get()
                    .get(key)
                    .unwrap()
                    .listen_shutdown_tx
                    .send(());
                self.domain_config_listen_map.get_mut().remove(key);
            }
        }

        let mut key_domain_config_listens = Vec::new();
        for (key, domain_config_listen) in &domain_core_conf.domain_config_listen_map {
            let (domain_config_listen, listen_shutdown_tx) = {
                let domain_config_listen_map_borrow = self.domain_config_listen_map.get();
                let domain_listen = domain_config_listen_map_borrow.get(key);
                if domain_listen.is_none() {
                    key_domain_config_listens.push((key, domain_config_listen));
                    continue;
                }

                let domain_listen = domain_listen.unwrap();
                let domain_config_listen = domain_config::DomainConfig::merger(
                    &domain_listen.domain_config_listen,
                    domain_config_listen.clone(),
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
            self.domain_config_listen_map.get_mut().insert(
                key.clone(),
                DomainListen {
                    domain_config_listen: Arc::new(domain_config_listen),
                    listen_shutdown_tx,
                },
            );
        }

        for (key, domain_config_listen) in key_domain_config_listens {
            let (listen_shutdown_tx, _) = broadcast::channel(100);
            let listen_server = domain_config_listen.listen_server.clone();

            let domain_listen = DomainListen {
                domain_config_listen: Arc::new(domain_config_listen.clone()),
                listen_shutdown_tx: listen_shutdown_tx.clone(),
            };
            self.domain_config_listen_map
                .get_mut()
                .insert(key.clone(), domain_listen);

            let domain_config_listen_map = self.domain_config_listen_map.clone();

            let domain_context = self.context.clone();
            let key = key.clone();
            let ms = ms.clone();

            self.executor._start(
                #[cfg(feature = "anyspawn-count")]
                None,
                move |executors| async move {
                    let domain_server = domain_server::DomainServer::new(
                        ms,
                        executors,
                        listen_shutdown_tx,
                        listen_server,
                        key,
                        domain_config_listen_map,
                        domain_context,
                    )
                    .map_err(|e| anyhow!("err:DomainServer::new => e:{}", e))?;
                    domain_server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:domain_server.start => e:{}", e))?;
                    Ok(())
                },
            );
        }
        Ok(())
    }

    async fn stop(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataStop>();
        scopeguard::defer! {
            log::info!("end domain stop flag:{}", value.name);
        }
        log::info!("start domain stop flag:{}", value.name);

        self.executor
            .stop(&value.name, value.is_fast_shutdown, value.shutdown_timeout)
            .await;
        Ok(())
    }
    async fn send(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataSend>();
        self.executor.send(&value.name, value.is_fast_shutdown);
        Ok(())
    }

    async fn wait(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataWait>();
        self.executor.wait(&value.name).await
    }
}
