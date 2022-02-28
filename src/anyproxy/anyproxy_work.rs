use crate::config::config_toml;
use crate::proxy::domain;
use crate::proxy::port;
use crate::proxy::proxy;
use any_tunnel::client;
use any_tunnel::server;
use any_tunnel2::client as client2;
use any_tunnel2::server as server2;
use awaitgroup::Worker;
use tokio::sync::broadcast;

pub struct AnyproxyWork {
    executor: async_executors::TokioCt,
    shutdown_thread_tx: broadcast::Sender<bool>,
    config_tx: broadcast::Sender<config_toml::ConfigToml>,
    group_version: i32,
    tunnel_clients: client::Client,
    tunnel_servers: server::Server,
    tunnel2_clients: client2::Client,
    tunnel2_servers: server2::Server,
}

impl AnyproxyWork {
    pub fn new(
        executor: async_executors::TokioCt,
        shutdown_thread_tx: broadcast::Sender<bool>,
        config_tx: broadcast::Sender<config_toml::ConfigToml>,
        group_version: i32,
        tunnel_clients: client::Client,
        tunnel_servers: server::Server,
        tunnel2_clients: client2::Client,
        tunnel2_servers: server2::Server,
    ) -> anyhow::Result<AnyproxyWork> {
        Ok(AnyproxyWork {
            executor,
            shutdown_thread_tx,
            config_tx,
            group_version,
            tunnel_clients,
            tunnel_servers,
            tunnel2_clients,
            tunnel2_servers,
        })
    }

    pub async fn start(
        &mut self,
        config: config_toml::ConfigToml,
        run_thread_wait_group_worker: Worker,
    ) -> anyhow::Result<()> {
        // log::info!(
        //     "work group_version:{} config:{:?}",
        //     self.group_version,
        //     config
        // );

        let mut proxys: Vec<Box<dyn proxy::Proxy>> = vec![
            Box::new(port::Port::new(
                self.executor.clone(),
                self.group_version,
                self.tunnel_clients.clone(),
                self.tunnel_servers.clone(),
                self.tunnel2_clients.clone(),
                self.tunnel2_servers.clone(),
            )?),
            Box::new(domain::Domain::new(
                self.executor.clone(),
                self.group_version,
                self.tunnel_clients.clone(),
                self.tunnel_servers.clone(),
                self.tunnel2_clients.clone(),
                self.tunnel2_servers.clone(),
            )?),
        ];

        for proxy in proxys.iter_mut() {
            if let Err(e) = proxy
                .start(&config)
                .await
                .map_err(|e| anyhow::anyhow!("err:start proxy => e:{}", e))
            {
                run_thread_wait_group_worker.error();
                return Err(e);
            }
        }
        run_thread_wait_group_worker.add();

        let mut shutdown_thread_rx = self.shutdown_thread_tx.subscribe();
        let mut config_rx = self.config_tx.subscribe();
        loop {
            tokio::select! {
                biased;
                config = self.config_receiver(&mut config_rx) => {
                    if config.is_err() {
                        continue;
                    }
                    let config = config?;
                    for proxy in proxys.iter_mut() {
                        if let Err(e) = proxy.start(&config).await
                        .map_err(|e| anyhow::anyhow!("err:proxy.start => e:{}", e)) {
                            log::error!("{}", e);
                            continue;
                        }
                    }
                }
                is_fast_shutdown = shutdown_thread_rx.recv() => {
                    let is_fast_shutdown = if is_fast_shutdown.is_err() {
                        true
                    } else {
                        is_fast_shutdown.unwrap()
                    };

                    for proxy in proxys.iter_mut() {
                        if let Err(e) = proxy.stop(is_fast_shutdown).await {
                            log::error!("err:proxy.stop => e:{}", e);
                        }
                    }
                    break;
                }
                else => {
                    return Err(anyhow::anyhow!("err:config_receiver"));
                }
            }
        }
        for proxy in proxys.iter_mut() {
            if let Err(e) = proxy.wait().await {
                log::error!("err:proxy.wait => e:{}", e);
            }
        }
        Ok(())
    }

    pub async fn config_receiver(
        &self,
        config_rx: &mut broadcast::Receiver<config_toml::ConfigToml>,
    ) -> anyhow::Result<config_toml::ConfigToml> {
        loop {
            let config = config_rx.recv().await?;
            // log::info!(
            //     "reload group_version:{} config:{:?}",
            //     self.group_version,
            //     config
            // );
            return Ok(config);
        }
    }
}
