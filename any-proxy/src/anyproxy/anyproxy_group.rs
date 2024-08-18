use super::anyproxy_work;
use crate::util::default_config;
use any_base::executor_local_spawn::_block_on;
use any_base::executor_local_spawn::{ExecutorLocalSpawn, ExecutorsLocal};
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::thread_pool::ThreadPool;
use any_base::{executor_local_spawn, DropMsExecutor};
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
#[cfg(unix)]
use rlimit::{setrlimit, Resource};
use tokio::sync::broadcast;

pub struct AnyproxyGroup {
    executor: ExecutorLocalSpawn,
    group_version: i32,
    config_tx: Option<broadcast::Sender<(module::Modules, WaitGroup)>>,
    thread_pool: Option<ThreadPool>,
    ms: Option<module::Modules>,
    worker_threads: usize,
    shutdown_timeout: u64,
    reload_count: usize,
    pub ms_tx: async_channel::Sender<Modules>,
}

impl AnyproxyGroup {
    pub fn new(
        group_version: i32,
        executor: ExecutorLocalSpawn,
        ms: Option<module::Modules>,
        ms_tx: async_channel::Sender<module::Modules>,
    ) -> Result<AnyproxyGroup> {
        Ok(AnyproxyGroup {
            group_version,
            config_tx: None,
            thread_pool: None,
            executor,
            ms,
            worker_threads: 0,
            shutdown_timeout: 30,
            reload_count: 0,
            ms_tx,
        })
    }
    pub fn ms(&self) -> Option<module::Modules> {
        self.ms.clone()
    }
    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub async fn start(&mut self) -> Result<()> {
        log::trace!(target: "main", "anyproxy_group start");
        log::info!("init_master_ms");
        let ms = self.ms();
        let ms: Result<module::Modules> = tokio::task::spawn_blocking(move || {
            executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
                let mut ms = module::Modules::new(ms, false);
                ms.parse_module_config(&file_name, None)
                    .await
                    .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;

                Ok(ms)
            })
        })
        .await
        .map_err(|e| anyhow!("err:parse_module_config => e:{}", e))?;
        let mut ms = ms?;
        if self.ms.is_none() {
            ms.ms_tx = Some(self.ms_tx.clone());
        }

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
        let dmse = DropMsExecutor::new(self.executor.clone(), ms_executor, shutdown_timeout);
        ms.init_master_thread(dmse)
            .await
            .map_err(|e| anyhow!("err:init_master_thread => e:{}", e))?;
        log::info!("init_master_thread");

        self.shutdown_timeout = shutdown_timeout;

        self.ms = Some(ms.clone());

        #[cfg(unix)]
        {
            let soft = common_conf.max_open_file_limit;
            let hard = soft;
            setrlimit(Resource::NOFILE, soft, hard)
                .map_err(|e| anyhow!("err:setrlimit => soft:{}, hard:{}, e:{}", soft, hard, e))?;

            use crate::util::util;
            util::memlock_rlimit(
                common_conf.memlock_rlimit_curr,
                common_conf.memlock_rlimit_max,
            )
            .map_err(|e| anyhow!("err:setrlimit => e:{}", e))?;
        }

        let (_config_tx, _) = broadcast::channel(100);
        let mut _worker_threads = common_conf.worker_threads;
        let mut _block_on_worker_threads = common_conf.worker_threads;
        #[cfg(not(unix))]
        {
            _worker_threads = 1;
        }

        #[cfg(unix)]
        {
            if common_conf.reuseport {
                _block_on_worker_threads = 1;
            } else {
                _worker_threads = 1;
            }
        }
        let worker_threads_blocking = common_conf.worker_threads_blocking;
        log::info!("worker_threads:{}", _worker_threads);
        log::info!("block_on_worker_threads:{}", _block_on_worker_threads);
        log::info!("worker_threads_blocking:{}", worker_threads_blocking);
        let config_tx = _config_tx.clone();
        let thread_pool = ThreadPool::new(
            _worker_threads,
            common_conf.cpu_affinity,
            self.group_version,
        );
        let mut thread_pool_wait_run = thread_pool.thread_pool_wait_run();
        thread_pool_wait_run._start(move |async_context| {
            log::debug!(target: "main",
                "group_version:{}, cpu_affinity:{}, thread_id:{:?}",
                async_context.group_version,
                async_context.cpu_affinity,
                async_context.thread_id
            );
            _block_on(
                _block_on_worker_threads,
                worker_threads_blocking,
                move |executor| async move {
                    let mut anyproxy_work =
                        anyproxy_work::AnyproxyWork::new(executor, config_tx)
                            .map_err(|e| anyhow!("err:AnyproxyWork::new => e:{}", e))?;
                    anyproxy_work.start(async_context, ms).await
                },
            )
        });

        thread_pool_wait_run
            .wait_run()
            .await
            .map_err(|e| anyhow!("err:thread_pool.wait_start => err: {}", e))?;

        self.config_tx = Some(_config_tx);
        self.thread_pool = Some(thread_pool);
        self.worker_threads = _worker_threads;

        Ok(())
    }

    pub async fn reload(&mut self, _executors: ExecutorsLocal) {
        self.reload_count += 1;
        log::info!(
            "reload start group_version:{}, reload_count:{}",
            self.group_version,
            self.reload_count
        );
        let ms = self.ms();
        let ret = tokio::task::spawn_blocking(move || {
            executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                let anyproxy_conf_full_path = default_config::ANYPROXY_CONF_FULL_PATH.get();
                let file_name = anyproxy_conf_full_path.as_str();

                let mut ms = module::Modules::new(ms, false);
                let ret = ms
                    .parse_module_config(file_name, None)
                    .await
                    .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e));

                (ret, ms)
            })
        })
        .await;
        let (ret, ms) = if let Err(e) = ret {
            (Err(anyhow::anyhow!("{}", e)), None)
        } else {
            let (ret, ms) = ret.unwrap();
            (ret, Some(ms))
        };
        match ret {
            Err(e) => {
                log::error!(
                    "err:reload config parse_module_config => group_version:{} e:{}",
                    self.group_version,
                    e
                );
                return;
            }
            _ => {}
        };
        let ms = ms.unwrap();

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
        let dmse = DropMsExecutor::new(self.executor.clone(), ms_executor, shutdown_timeout);
        let ret = ms
            .init_master_thread(dmse)
            .await
            .map_err(|e| anyhow!("err:init_master_thread => e:{}", e));
        log::info!("init_master_thread");
        if let Err(e) = ret {
            log::error!(
                "err:reload config init_master_thread => group_version:{} e:{}",
                self.group_version,
                e
            );
            return;
        }

        self.shutdown_timeout = shutdown_timeout;
        self.ms = Some(ms.clone());

        #[cfg(unix)]
        {
            let common_conf = common_core::main_conf(&ms).await;

            let soft = common_conf.max_open_file_limit;
            let hard = soft;
            if let Err(e) = setrlimit(Resource::NOFILE, soft, hard)
                .map_err(|e| anyhow!("err:setrlimit => soft:{}, hard:{}, e:{}", soft, hard, e))
            {
                log::error!(":{}", e);
            }
        }
        let wait_group = WaitGroup::new();
        let _ = self
            .config_tx
            .as_ref()
            .unwrap()
            .send((ms, wait_group.clone()));
        let _ = wait_group.wait_complete(self.worker_threads).await;
        log::info!(
            "reload ok group_version:{}, reload_count:{}",
            self.group_version,
            self.reload_count
        );
    }

    pub async fn stop(&mut self, is_fast_shutdown: bool) {
        self.thread_pool
            .as_ref()
            .unwrap()
            .stop(is_fast_shutdown, self.shutdown_timeout)
            .await;
        self.ms.take();
    }

    pub async fn check(group_version: i32, executor: ExecutorLocalSpawn) -> Result<()> {
        log::info!("check start group_version:{}", group_version);
        let ret: Result<Modules> = tokio::task::spawn_blocking(move || {
            executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                let anyproxy_conf_full_path = default_config::ANYPROXY_CONF_FULL_PATH.get();
                let file_name = anyproxy_conf_full_path.as_str();
                let mut ms = module::Modules::new(None, false);
                ms.parse_module_config(&file_name, None)
                    .await
                    .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;
                Ok(ms)
            })
        })
        .await?;

        match ret {
            Err(e) => {
                log::error!(
                    "err:check config parse_module_config => group_version:{} e:{}",
                    group_version,
                    e
                );
                return Err(anyhow::anyhow!(
                    "err:check config parse_module_config => group_version:{} e:{}",
                    group_version,
                    e
                ));
            }
            Ok(ms) => {
                use crate::config::common_core;
                let common_conf = common_core::main_conf(&ms).await;
                let shutdown_timeout = common_conf.shutdown_timeout;

                let ms_executor = ExecutorLocalSpawn::new(executor.executors());
                let dmse = DropMsExecutor::new(executor.clone(), ms_executor, shutdown_timeout);
                let ret = ms
                    .init_master_thread(dmse)
                    .await
                    .map_err(|e| anyhow!("err:init_master_thread => e:{}", e));
                if let Err(e) = ret {
                    log::error!(
                        "err:check config init_master_thread => group_version:{} e:{}",
                        group_version,
                        e
                    );
                    return Err(anyhow::anyhow!(
                        "err:check config init_master_thread => group_version:{} e:{}",
                        group_version,
                        e
                    ));
                }
            }
        };

        log::info!("check ok group_version:{}", group_version);
        Ok(())
    }
}
