use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::proxy::domain;
use crate::proxy::port;
use crate::proxy::proxy;
use crate::upstream::upstream;
use crate::TunnelClients;
use crate::Tunnels;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::thread_spawn::AsyncThreadContext;
use anyhow::anyhow;
use anyhow::Result;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct AnyproxyWork {
    executor_local_spawn: ExecutorLocalSpawn,
    tunnels: Tunnels,
    config_tx: broadcast::Sender<config_toml::ConfigToml>,
}

impl AnyproxyWork {
    pub fn new(
        executor_local_spawn: ExecutorLocalSpawn,
        tunnels: Tunnels,
        config_tx: broadcast::Sender<config_toml::ConfigToml>,
    ) -> Result<AnyproxyWork> {
        Ok(AnyproxyWork {
            executor_local_spawn,
            config_tx,
            tunnels,
        })
    }

    pub async fn start(
        &mut self,
        config: config_toml::ConfigToml,
        async_context: AsyncThreadContext,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<()> {
        let shutdown_timeout = config.common.shutdown_timeout;
        let mut proxys: Vec<Box<dyn proxy::Proxy>> = vec![
            Box::new(port::Port::new(
                self.executor_local_spawn.executors(),
                self.tunnels.clone(),
                #[cfg(feature = "anyproxy-ebpf")]
                ebpf_add_sock_hash.clone(),
                session_id.clone(),
            )?),
            Box::new(domain::Domain::new(
                self.executor_local_spawn.executors(),
                self.tunnels.clone(),
                #[cfg(feature = "anyproxy-ebpf")]
                ebpf_add_sock_hash.clone(),
                session_id.clone(),
            )?),
        ];

        let tunnel_clients = TunnelClients {
            client: self.tunnels.client.clone(),
            client2: self.tunnels.client2.clone(),
        };

        let mut ups_version = 1;

        let mut ups = upstream::Upstream::new(
            self.executor_local_spawn.executors(),
            tunnel_clients.clone(),
        )
        .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;

        if let Err(e) = ups
            .start(&config, ups_version)
            .await
            .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))
        {
            return Err(e);
        }

        let mut ups = Rc::new(ups);

        for proxy in proxys.iter_mut() {
            if let Err(e) = proxy
                .start(&config, ups.clone())
                .await
                .map_err(|e| anyhow!("err:start proxy => e:{}", e))
            {
                return Err(e);
            }
        }
        async_context.complete();
        let mut shutdown_thread_rx = async_context.shutdown_thread_tx.subscribe();
        let mut config_rx = self.config_tx.subscribe();
        let is_fast_shutdown = loop {
            tokio::select! {
                biased;
                config = self.config_receiver(&mut config_rx) => {
                    if config.is_err() {
                        continue;
                    }
                    let config = config?;

                    ups.stop("anyproxy_work stop ups", true, shutdown_timeout, ups_version).await?;

                    ups_version += 1;
                    let mut _ups = upstream::Upstream::new(self.executor_local_spawn.executors(),tunnel_clients.clone())
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
                ret = shutdown_thread_rx.recv() => {
                    let is_fast_shutdown = if ret.is_err() {
                        true
                    } else {
                        ret.unwrap()
                    };
                    ups.send("anyproxy_work send ups", is_fast_shutdown).await?;
                    for proxy in proxys.iter_mut() {
                        if let Err(e) = proxy.send("anyproxy_work send", is_fast_shutdown).await {
                            log::error!("err:proxy.stop => e:{}", e);
                        }
                    }
                    break is_fast_shutdown;
                }
                else => {
                    return Err(anyhow!("err:config_receiver"));
                }
            }
        };
        ups.stop(
            "anyproxy_work stop ups",
            is_fast_shutdown,
            shutdown_timeout,
            ups_version,
        )
        .await?;
        for proxy in proxys.iter_mut() {
            if let Err(e) = proxy
                .stop(
                    "anyproxy_work stop proxy",
                    is_fast_shutdown,
                    shutdown_timeout,
                )
                .await
            {
                log::error!("err:proxy.stop => e:{}", e);
            }
        }
        self.executor_local_spawn
            .stop("anyproxy_work stop", is_fast_shutdown, shutdown_timeout)
            .await;
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
