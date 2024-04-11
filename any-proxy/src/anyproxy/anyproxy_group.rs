use super::anyproxy_work;
use crate::util::default_config;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::executor_local_spawn::_block_on;
use any_base::module::module;
use any_base::thread_pool::ThreadPool;
use anyhow::anyhow;
use anyhow::Result;
#[cfg(unix)]
use rlimit::{setrlimit, Resource};
use tokio::sync::broadcast;

pub struct AnyproxyGroup {
    group_version: i32,
    config_tx: Option<broadcast::Sender<module::Modules>>,
    thread_pool: Option<ThreadPool>,
    ms: module::Modules,
}

impl AnyproxyGroup {
    pub fn new(group_version: i32, ms: module::Modules) -> Result<AnyproxyGroup> {
        Ok(AnyproxyGroup {
            group_version,
            config_tx: None,
            thread_pool: None,
            ms,
        })
    }
    pub fn ms(&self) -> module::Modules {
        self.ms.clone()
    }
    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub async fn start(&mut self) -> Result<()> {
        log::trace!("anyproxy_group start");
        let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
        let mut ms = module::Modules::new(Some(self.ms.clone()), false);
        ms.parse_module_config(&file_name, None)
            .await
            .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;
        self.ms = ms.clone();
        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;

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
            log::debug!(
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
                    anyproxy_work
                        .start(async_context, ms)
                        .await
                        .map_err(|e| anyhow!("err:anyproxy_work.start => e:{}", e))?;
                    Ok(())
                },
            )?;
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

    pub async fn reload(&mut self, _executors: ExecutorsLocal) {
        let anyproxy_conf_full_path = default_config::ANYPROXY_CONF_FULL_PATH.get();
        let file_name = anyproxy_conf_full_path.as_str();

        let mut ms = module::Modules::new(Some(self.ms.clone()), false);
        let ret = ms
            .parse_module_config(file_name, None)
            .await
            .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e));
        match ret {
            Err(e) => {
                log::error!(
                    "err:reload config => group_version:{} e:{}",
                    self.group_version,
                    e
                );
                return;
            }
            _ => {}
        };
        self.ms = ms.clone();

        log::info!("reload ok group_version:{}", self.group_version);

        #[cfg(unix)]
        {
            use crate::config::common_core;
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
            .await
    }

    pub async fn check(group_version: i32, _executors: ExecutorsLocal) -> Result<()> {
        let anyproxy_conf_full_path = default_config::ANYPROXY_CONF_FULL_PATH.get();
        let file_name = anyproxy_conf_full_path.as_str();

        let mut ms = module::Modules::new(None, false);
        let ret = ms
            .parse_module_config(&file_name, None)
            .await
            .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e));
        match ret {
            Err(e) => {
                log::error!(
                    "err:check config => group_version:{} e:{}",
                    group_version,
                    e
                );
                return Err(anyhow::anyhow!(
                    "err:check config => group_version:{} e:{}",
                    group_version,
                    e
                ));
            }
            _ => {}
        };

        log::info!("check ok group_version:{}", group_version);
        Ok(())
    }
}
