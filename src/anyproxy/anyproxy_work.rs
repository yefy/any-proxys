use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::ebpf_sockhash;
use crate::proxy::domain;
use crate::proxy::port;
use crate::proxy::proxy;
use crate::upstream::upstream;
use crate::Executors;
use crate::TunnelClients;
use crate::Tunnels;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::Worker;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct AnyproxyWork {
    executors: Executors,
    tunnels: Tunnels,
    config_tx: broadcast::Sender<config_toml::ConfigToml>,
}

impl AnyproxyWork {
    pub fn new(
        executors: Executors,
        tunnels: Tunnels,
        config_tx: broadcast::Sender<config_toml::ConfigToml>,
    ) -> Result<AnyproxyWork> {
        Ok(AnyproxyWork {
            executors,
            config_tx,
            tunnels,
        })
    }

    pub async fn start(
        &mut self,
        config: config_toml::ConfigToml,
        run_thread_wait_group_worker: Worker,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
    ) -> Result<()> {
        // log::info!(
        //     "work group_version:{} config:{:?}",
        //     self.group_version,
        //     config
        // );

        let mut proxys: Vec<Box<dyn proxy::Proxy>> = vec![
            Box::new(port::Port::new(
                self.executors.clone(),
                self.tunnels.clone(),
                #[cfg(feature = "anyproxy-ebpf")]
                ebpf_add_sock_hash.clone(),
            )?),
            Box::new(domain::Domain::new(
                self.executors.clone(),
                self.tunnels.clone(),
                #[cfg(feature = "anyproxy-ebpf")]
                ebpf_add_sock_hash.clone(),
            )?),
        ];

        let tunnel_clients = TunnelClients {
            client: self.tunnels.client.clone(),
            client2: self.tunnels.client2.clone(),
        };

        let mut ups_version = 1;

        let mut ups = upstream::Upstream::new(self.executors.clone(), tunnel_clients.clone())
            .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;
        ups.start(&config, ups_version)
            .await
            .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;

        let mut ups = Rc::new(ups);

        for proxy in proxys.iter_mut() {
            if let Err(e) = proxy
                .start(&config, ups.clone())
                .await
                .map_err(|e| anyhow!("err:start proxy => e:{}", e))
            {
                run_thread_wait_group_worker.error();
                return Err(e);
            }
        }
        run_thread_wait_group_worker.add();

        let mut shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();
        let mut config_rx = self.config_tx.subscribe();
        loop {
            tokio::select! {
                biased;
                config = self.config_receiver(&mut config_rx) => {
                    if config.is_err() {
                        continue;
                    }
                    let config = config?;

                    ups.stop(true).await?;
                    ups.wait(ups_version).await?;

                    ups_version += 1;
                    let mut _ups = upstream::Upstream::new(self.executors.clone(),tunnel_clients.clone())
                        .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;
                    _ups.start(&config, ups_version).await.map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;

                    ups = Rc::new(_ups);

                    for proxy in proxys.iter_mut() {
                        if let Err(e) = proxy.start(&config, ups.clone()).await
                        .map_err(|e| anyhow!("err:proxy.start => e:{}", e)) {
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
                    ups.stop(true).await?;
                    for proxy in proxys.iter_mut() {
                        if let Err(e) = proxy.stop(is_fast_shutdown).await {
                            log::error!("err:proxy.stop => e:{}", e);
                        }
                    }
                    break;
                }
                else => {
                    return Err(anyhow!("err:config_receiver"));
                }
            }
        }
        ups.wait(ups_version).await?;
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
    ) -> Result<config_toml::ConfigToml> {
        loop {
            let config = config_rx
                .recv()
                .await
                .map_err(|e| anyhow!("err:config_rx.recv => e:{}", e))?;
            // log::info!(
            //     "reload group_version:{} config:{:?}",
            //     self.group_version,
            //     config
            // );
            return Ok(config);
        }
    }
}
