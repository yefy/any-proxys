use super::anyproxy_work;
use crate::config::config;
use crate::config::config_toml;
use crate::config::config_toml::ConfigToml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::proxy::domain_config;
use crate::proxy::port_config;
use crate::proxy::proxy;
use crate::upstream::upstream;
use crate::TunnelClients;
use crate::Tunnels;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::executor_local_spawn::_block_on;
use any_base::thread_pool::ThreadPool;
use any_tunnel::client;
use any_tunnel2::client as client2;
use any_tunnel2::tunnel as tunnel2;
use anyhow::anyhow;
use anyhow::Result;
#[cfg(unix)]
use rlimit::{setrlimit, Resource};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct AnyproxyGroup {
    group_version: i32,
    config_tx: Option<broadcast::Sender<config_toml::ConfigToml>>,
    thread_pool: Option<ThreadPool>,
}

impl AnyproxyGroup {
    pub fn new(group_version: i32) -> Result<AnyproxyGroup> {
        Ok(AnyproxyGroup {
            group_version,
            config_tx: None,
            thread_pool: None,
        })
    }

    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub async fn start(
        &mut self,
        config: Option<ConfigToml>,
        peer_stream_max_len: Arc<AtomicUsize>,
        tunnels: Tunnels,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<()> {
        let config = if config.is_none() {
            config::Config::new().map_err(|e| anyhow!("err:Config::new() => e:{}", e))?
        } else {
            config.unwrap()
        };

        #[cfg(unix)]
        {
            let soft = config.common.max_open_file_limit;
            let hard = soft;
            setrlimit(Resource::NOFILE, soft, hard)
                .map_err(|e| anyhow!("err:setrlimit => soft:{}, hard:{}, e:{}", soft, hard, e))?;

            use crate::util::util;
            let memlock_rlimit = config.common.memlock_rlimit.clone();
            util::memlock_rlimit(memlock_rlimit.curr, memlock_rlimit.max)
                .map_err(|e| anyhow!("err:setrlimit => e:{}", e))?;
        }

        //reload 的时候不能改变peer_stream_max_len 使用swap获取最新的值
        peer_stream_max_len.swap(config.tunnel2.tunnel2_max_connect, Ordering::Relaxed);

        let (_config_tx, _) = broadcast::channel(100);
        log::info!("tunnel2_max_connect:{}", config.tunnel2.tunnel2_max_connect);

        let config_tx = _config_tx.clone();
        let thread_pool = ThreadPool::new(
            config.common.worker_threads,
            config.common.cpu_affinity,
            self.group_version,
        );
        let mut thread_pool_wait_run = thread_pool.thread_pool_wait_run();
        thread_pool_wait_run._start(move |async_context| {
            log::debug!(
                "group_version:{}, cpu_affinity:{}, thread_id:{:?}",
                async_context.group_version,
                async_context.cpu_affinity,
                async_context.thread_id
            );
            _block_on(move |executor_local_spawn| async move {
                let mut anyproxy_work =
                    anyproxy_work::AnyproxyWork::new(executor_local_spawn, tunnels, config_tx)
                        .map_err(|e| anyhow!("err:AnyproxyWork::new => e:{}", e))?;
                anyproxy_work
                    .start(
                        config,
                        async_context,
                        #[cfg(feature = "anyproxy-ebpf")]
                        ebpf_add_sock_hash,
                        session_id,
                    )
                    .await
                    .map_err(|e| anyhow!("err:anyproxy_work.start => e:{}", e))?;
                Ok(())
            })?;
            Ok(())
        });

        thread_pool_wait_run
            .wait_run()
            .await
            .map_err(|e| anyhow!("err:thread_pool.wait_start => err: {}", e))?;

        self.config_tx = Some(_config_tx);
        self.thread_pool = Some(thread_pool);

        Ok(())
    }

    pub async fn reload(&self, peer_stream_max_len: Arc<AtomicUsize>, executors: ExecutorsLocal) {
        let config = AnyproxyGroup::check(self.group_version, executors).await;
        let config = match config {
            Err(e) => {
                log::error!(
                    "err:reload config => group_version:{} e:{}",
                    self.group_version,
                    e
                );
                return;
            }
            Ok(config) => config,
        };
        peer_stream_max_len.swap(config.tunnel2.tunnel2_max_connect, Ordering::Relaxed);
        // log::info!(
        //     "reload group_version:{} config:{:?}",
        //     self.group_version,
        //     config
        // );
        log::info!("reload ok group_version:{}", self.group_version);

        #[cfg(unix)]
        {
            let soft = config.common.max_open_file_limit;
            let hard = soft;
            if let Err(e) = setrlimit(Resource::NOFILE, soft, hard)
                .map_err(|e| anyhow!("err:setrlimit => soft:{}, hard:{}, e:{}", soft, hard, e))
            {
                log::error!(":{}", e);
            }
        }
        let _ = self.config_tx.as_ref().unwrap().send(config);
    }

    pub async fn check(
        group_version: i32,
        executors: ExecutorsLocal,
    ) -> Result<config_toml::ConfigToml> {
        let config = config::Config::new().map_err(|e| anyhow!("err:Config::new() => e:{}", e));
        let config = match config {
            Err(e) => {
                log::error!(
                    "err:check config => group_version:{} e:{}",
                    group_version,
                    e
                );
                return Err(e);
            }
            Ok(config) => config,
        };

        let client2 = tunnel2::Tunnel::start_thread(1).await;
        let client2 = client2::Client::new(client2, Arc::new(AtomicUsize::new(1)));
        let client = client::Client::new();

        let proxy_configs: Vec<Box<dyn proxy::Config>> = vec![
            Box::new(port_config::PortConfig::new()?),
            Box::new(domain_config::DomainConfig::new()?),
        ];

        let tunnel_clients = TunnelClients { client, client2 };

        let mut ups = upstream::Upstream::new(executors, tunnel_clients.clone())
            .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;
        ups.parse_config(&config)
            .await
            .map_err(|e| anyhow!("err:Upstream::new => e:{}", e))?;

        let ups = Rc::new(ups);

        for proxy_config in proxy_configs.iter() {
            if let Err(e) = proxy_config
                .parse(&config, ups.clone(), tunnel_clients.clone())
                .await
            {
                log::error!(
                    "err:check => group_version:{} config:{:?} e:{}",
                    group_version,
                    config,
                    e
                );
                return Err(e);
            }
        }

        //log::info!("check group_version:{} config:{:?}", group_version, config);
        log::info!("check ok group_version:{}", group_version);
        Ok(config)
    }

    pub async fn stop(&self, is_fast_shutdown: bool) {
        self.thread_pool
            .as_ref()
            .unwrap()
            .stop(is_fast_shutdown, 10)
            .await
    }
}
