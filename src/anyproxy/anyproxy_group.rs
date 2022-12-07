use super::anyproxy_work;
use crate::config::config;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::ebpf_sockhash;
use crate::proxy::domain_config;
use crate::proxy::port_config;
use crate::proxy::proxy;
use crate::upstream::upstream;
use crate::Executors;
use crate::TunnelClients;
use crate::Tunnels;
use any_tunnel::client;
use any_tunnel2::client as client2;
use any_tunnel2::tunnel as tunnel2;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
#[cfg(unix)]
use rlimit::{setrlimit, Resource};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::sync::broadcast;

pub struct AnyproxyGroup {
    group_version: i32,
    shutdown_thread_tx: Option<broadcast::Sender<bool>>,
    config_tx: Option<broadcast::Sender<config_toml::ConfigToml>>,
    thread_handles: Option<Vec<std::thread::JoinHandle<()>>>,
    thread_wait_group: Option<WaitGroup>,
}

impl AnyproxyGroup {
    pub fn new(group_version: i32) -> Result<AnyproxyGroup> {
        Ok(AnyproxyGroup {
            group_version,
            shutdown_thread_tx: None,
            config_tx: None,
            thread_handles: None,
            thread_wait_group: None,
        })
    }

    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub async fn start(
        &mut self,
        peer_stream_max_len: Arc<AtomicUsize>,
        tunnels: Tunnels,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
    ) -> Result<()> {
        let config = config::Config::new().map_err(|e| anyhow!("err:Config::new() => e:{}", e))?;
        // log::info!(
        //     "start group_version:{} config:{:?}",
        //     self.group_version,
        //     config
        // );

        #[cfg(unix)]
        {
            let soft = config.common.max_open_file_limit;
            let hard = soft;
            setrlimit(Resource::NOFILE, soft, hard)
                .map_err(|e| anyhow!("err:setrlimit => soft:{}, hard:{}, e:{}", soft, hard, e))?;
        }

        //reload 的时候不能改变peer_stream_max_len 使用swap获取最新的值
        peer_stream_max_len.swap(config.tunnel2.tunnel2_max_connect, Ordering::Relaxed);

        let mut worker_thread_core_ids = Vec::with_capacity(50);
        let mut worker_threads = config.common.worker_threads as i32;
        // 读取想要线程数的cpu id
        while worker_threads > 0 {
            let thread_core_ids = core_affinity::get_core_ids();
            if thread_core_ids.is_none() {
                break;
            }
            let thread_core_ids = thread_core_ids.unwrap();
            if thread_core_ids.len() <= 0 {
                break;
            }
            worker_threads -= thread_core_ids.len() as i32;
            worker_thread_core_ids.extend(thread_core_ids.iter());
        }

        let cpu_affinity = config.common.cpu_affinity;
        let group_version = self.group_version;

        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let (config_tx, _) = broadcast::channel(100);
        let thread_wait_group = WaitGroup::new();
        let run_thread_wait_group = WaitGroup::new();
        log::info!("tunnel2_max_connect:{}", config.tunnel2.tunnel2_max_connect);
        log::info!("worker_threads:{}", config.common.worker_threads);
        log::info!("group_version:{}", group_version);
        // 创建线程
        let thread_handles = (0..config.common.worker_threads)
            .map(|index| {
                let config = config.clone();
                let shutdown_thread_tx = shutdown_thread_tx.clone();
                let config_tx = config_tx.clone();
                let worker_thread_core_ids = worker_thread_core_ids.clone();
                let tunnels = tunnels.clone();
                #[cfg(feature = "anyproxy-ebpf")]
                let ebpf_add_sock_hash = ebpf_add_sock_hash.clone();

                let worker_thread = thread_wait_group.worker().add();
                let run_thread_wait_group_worker = run_thread_wait_group.worker();
                thread::spawn(move || {
                    let thread_id = thread::current().id();
                    scopeguard::defer! {
                        log::info!("stop thread group_version:{} index:{}, worker_threads_id:{:?}", group_version, index, thread_id);
                        worker_thread.done();
                    }
                    log::info!("start thread group_version:{} index:{}, worker_threads_id:{:?}", group_version, index, thread_id);


                    if cpu_affinity && index < worker_thread_core_ids.len() {
                        log::info!(
                            "cpu_affinity thread group_version:{}, index:{} worker_threads_id:{:?} affinity {:?}",
                            group_version,
                            index,
                            thread_id,
                            worker_thread_core_ids[index]
                        );
                        core_affinity::set_for_current(worker_thread_core_ids[index]);
                    }



                    let executor = async_executors::TokioCtBuilder::new().build().unwrap();
                    let executors = Executors{
                        executor,
                        shutdown_thread_tx,
                        group_version,
                        thread_id,
                    };
                    executors.executor.clone().block_on(async {
                        let ret: Result<()> = async {
                            let mut anyproxy_work = anyproxy_work::AnyproxyWork::new(
                                executors,
                                tunnels,
                                config_tx,
                            ).map_err(|e| anyhow!("err:anyproxy_work::AnyproxyWork::new => e:{}", e))?;
                            anyproxy_work.start(config, run_thread_wait_group_worker,
                                                #[cfg(feature = "anyproxy-ebpf")]
                                                ebpf_add_sock_hash).await.map_err(|e| anyhow!("err:anyproxy_work.start => e:{}", e))?;
                            Ok(())
                        }.await;
                        ret.unwrap_or_else(|e| log::error!("err:AnyproxyWork => e:{}", e));
                    });
                })
            })
            .collect::<Vec<_>>();

        self.shutdown_thread_tx = Some(shutdown_thread_tx);
        self.config_tx = Some(config_tx);
        self.thread_handles = Some(thread_handles);
        self.thread_wait_group = Some(thread_wait_group);

        if run_thread_wait_group
            .wait_or_error(config.common.worker_threads)
            .await
        {
            return Err(anyhow!("err:AnyproxyGroup.start"));
        }

        Ok(())
    }

    pub async fn reload(&self, peer_stream_max_len: Arc<AtomicUsize>) {
        let executor = async_executors::TokioCtBuilder::new().build().unwrap();
        let config = AnyproxyGroup::check(self.group_version, executor).await;
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
        executor: async_executors::TokioCt,
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
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let executors = Executors {
            executor,
            shutdown_thread_tx,
            group_version: 0,
            thread_id: thread::current().id(),
        };
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

    pub async fn stop(&mut self, is_fast_shutdown: bool) {
        log::info!("stop group_version:{}", self.group_version);
        let _ = self
            .shutdown_thread_tx
            .as_ref()
            .unwrap()
            .send(is_fast_shutdown);
        let mut count: i32 = 0;
        loop {
            tokio::select! {
                biased;
                _ = self.thread_wait_group.as_ref().unwrap().wait() =>  {
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    let _ = self.shutdown_thread_tx.as_ref().unwrap().send(is_fast_shutdown);
                    count += 1;
                    if count >= 60 {
                        log::warn!(
                        "stop timeout: group_version:{}",
                        self.group_version,
                        );
                        count = 0;
                    }
                },
                else => {
                    break;
                }
            }
        }

        self.thread_handles.take().map(|thread_handles| {
            log::info!("join group_version:{}", self.group_version);
            for handle in thread_handles.into_iter() {
                handle.join().unwrap();
            }
            log::info!("done group_version:{}", self.group_version);
        });
    }
}
