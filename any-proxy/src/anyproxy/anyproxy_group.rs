use super::anyproxy_work;
use crate::util::default_config;
use crate::ModulesExecutor;
use any_base::executor_local_spawn;
use any_base::executor_local_spawn::_block_on;
use any_base::executor_local_spawn::{ExecutorLocalSpawn, ExecutorsLocal};
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::thread_pool::ThreadPool;
use anyhow::anyhow;
use anyhow::Result;
#[cfg(unix)]
use rlimit::{setrlimit, Resource};
use tokio::sync::broadcast;

pub struct AnyproxyGroup {
    executor: ExecutorLocalSpawn,
    group_version: i32,
    config_tx: Option<broadcast::Sender<module::Modules>>,
    thread_pool: Option<ThreadPool>,
    mse: ModulesExecutor,
}

impl AnyproxyGroup {
    pub fn new(
        group_version: i32,
        mse: ModulesExecutor,
        executor: ExecutorLocalSpawn,
    ) -> Result<AnyproxyGroup> {
        Ok(AnyproxyGroup {
            group_version,
            config_tx: None,
            thread_pool: None,
            mse,
            executor,
        })
    }
    pub fn ms(&self) -> module::Modules {
        self.mse.ms.clone()
    }
    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub async fn start(&mut self) -> Result<()> {
        log::trace!(target: "main", "anyproxy_group start");
        let ms = self.ms();
        let ms: Result<module::Modules> = tokio::task::spawn_blocking(move || {
            executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
                let mut ms = module::Modules::new(Some(ms), false);
                ms.parse_module_config(&file_name, None)
                    .await
                    .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;

                Ok(ms)
            })
        })
        .await
        .map_err(|e| anyhow!("err:parse_module_config => e:{}", e))?;
        let ms = ms.unwrap();

        let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
        ms.init_master_thread(self.executor.clone(), ms_executor.clone())
            .await
            .map_err(|e| anyhow!("err:init_master_thread => e:{}", e))?;

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let mse = ModulesExecutor::new(
            ms.clone(),
            self.executor.clone(),
            ms_executor,
            shutdown_timeout,
        );
        self.mse = mse;

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

        Ok(())
    }

    pub async fn reload(&mut self, _executors: ExecutorsLocal) {
        let ms = self.ms();
        let ret = tokio::task::spawn_blocking(move || {
            executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                let anyproxy_conf_full_path = default_config::ANYPROXY_CONF_FULL_PATH.get();
                let file_name = anyproxy_conf_full_path.as_str();

                let mut ms = module::Modules::new(Some(ms), false);
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
        let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
        let ret = ms
            .init_master_thread(self.executor.clone(), ms_executor.clone())
            .await
            .map_err(|e| anyhow!("err:init_master_thread => e:{}", e));
        if let Err(e) = ret {
            log::error!(
                "err:reload config init_master_thread => group_version:{} e:{}",
                self.group_version,
                e
            );
            return;
        }
        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let mse = ModulesExecutor::new(
            ms.clone(),
            self.executor.clone(),
            ms_executor,
            shutdown_timeout,
        );
        self.mse = mse;

        log::info!("reload ok group_version:{}", self.group_version);

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
        let _ = self.config_tx.as_ref().unwrap().send(ms);
    }

    pub async fn stop(&self, is_fast_shutdown: bool, shutdown_timeout: u64) {
        self.thread_pool
            .as_ref()
            .unwrap()
            .stop(is_fast_shutdown, shutdown_timeout)
            .await;
        self.mse.del_ms_executor();
    }

    pub async fn check(group_version: i32, executor: ExecutorLocalSpawn) -> Result<()> {
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
                let ms_executor = ExecutorLocalSpawn::new(executor.executors());
                let ret = ms
                    .init_master_thread(executor.clone(), ms_executor.clone())
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

                use crate::config::common_core;
                let common_conf = common_core::main_conf(&ms).await;
                let shutdown_timeout = common_conf.shutdown_timeout;
                let _mse =
                    ModulesExecutor::new(ms, executor.clone(), ms_executor, shutdown_timeout);
            }
        };

        log::info!("check ok group_version:{}", group_version);
        Ok(())
    }
}
