use super::port_config;
use super::port_server;
use crate::anyproxy::anyproxy_work::{
    AnyproxyWorkDataNew, AnyproxyWorkDataSend, AnyproxyWorkDataStop, AnyproxyWorkDataWait,
};
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
pub struct PortListen {
    pub port_config_listen: Arc<port_config::PortConfigListen>,
    pub listen_shutdown_tx: broadcast::Sender<()>,
}

pub struct Port {
    executor: ExecutorLocalSpawn,
    port_config_listen_map: ShareRw<HashMap<String, PortListen>>,
}

impl Port {
    pub fn new(value: typ::ArcUnsafeAny) -> Result<Port> {
        let value = value.get::<AnyproxyWorkDataNew>();
        let executor = ExecutorLocalSpawn::new(value.executors.clone());

        Ok(Port {
            executor,
            port_config_listen_map: ShareRw::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl module::Server for Port {
    async fn start(&mut self, ms: Modules, _value: ArcUnsafeAny) -> Result<()> {
        log::trace!(target: "main", "port start");
        use crate::config::port_core;
        let port_core_conf = port_core::main_conf_mut(&ms).await;

        if port_core_conf.port_config_listen_map.is_empty() {
            return Ok(());
        }
        let keys = self
            .port_config_listen_map
            .get()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys.iter() {
            if port_core_conf.port_config_listen_map.get(key).is_none() {
                let _ = self
                    .port_config_listen_map
                    .get()
                    .get(key)
                    .unwrap()
                    .listen_shutdown_tx
                    .send(());
                self.port_config_listen_map.get_mut().remove(key);
            }
        }

        let mut key_port_config_listens = Vec::new();
        for (key, port_config_listen) in &port_core_conf.port_config_listen_map {
            let (port_config_listen, listen_shutdown_tx) = {
                let port_config_listen_map_borrow = self.port_config_listen_map.get();
                let port_listen = port_config_listen_map_borrow.get(key);
                if port_listen.is_none() {
                    key_port_config_listens.push((key, port_config_listen));
                    continue;
                }

                let port_listen = port_listen.unwrap();
                let port_config_listen = port_config::PortConfig::merger(
                    &port_listen.port_config_listen,
                    port_config_listen.clone(),
                );
                if port_config_listen.is_err() {
                    continue;
                }
                let port_config_listen = port_config_listen?;
                (port_config_listen, port_listen.listen_shutdown_tx.clone())
            };
            self.port_config_listen_map.get_mut().insert(
                key.clone(),
                PortListen {
                    port_config_listen: Arc::new(port_config_listen),
                    listen_shutdown_tx,
                },
            );
        }

        for (key, port_config_listen) in key_port_config_listens {
            let (listen_shutdown_tx, _) = broadcast::channel(100);
            let listen_server = port_config_listen.listen_server.clone();

            let port_listen = PortListen {
                port_config_listen: Arc::new(port_config_listen.clone()),
                listen_shutdown_tx: listen_shutdown_tx.clone(),
            };
            self.port_config_listen_map
                .get_mut()
                .insert(key.clone(), port_listen);

            let port_config_listen_map = self.port_config_listen_map.clone();
            let key = key.clone();
            let ms = ms.clone();

            self.executor._start(
                #[cfg(feature = "anyspawn-count")]
                None,
                move |executors| async move {
                    let port_server = port_server::PortServer::new(
                        ms,
                        executors,
                        listen_shutdown_tx,
                        listen_server,
                        key,
                        port_config_listen_map,
                    )
                    .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    port_server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                    Ok(())
                },
            );
        }

        Ok(())
    }
    async fn stop(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataStop>();
        scopeguard::defer! {
            log::info!("end port stop flag:{}", value.name);
        }
        log::info!("start port stop flag:{}", value.name);

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
