use crate::util::default_config;
use crate::ModulesExecutor;
use any_base::executor_local_spawn;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::module::module;
use any_base::thread_spawn::AsyncThreadContext;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AnyproxyWorkDataNew {
    pub executors: ExecutorsLocal,
}

impl AnyproxyWorkDataNew {
    pub fn new(executors: ExecutorsLocal) -> Self {
        AnyproxyWorkDataNew { executors }
    }
}

pub struct AnyproxyWorkDataStart {}

impl AnyproxyWorkDataStart {
    pub fn new() -> Self {
        AnyproxyWorkDataStart {}
    }
}

pub struct AnyproxyWorkDataSend {
    pub name: String,
    pub is_fast_shutdown: bool,
}

impl AnyproxyWorkDataSend {
    pub fn new(name: String, is_fast_shutdown: bool) -> Self {
        AnyproxyWorkDataSend {
            name,
            is_fast_shutdown,
        }
    }
}

pub struct AnyproxyWorkDataStop {
    pub name: String,
    pub is_fast_shutdown: bool,
    pub shutdown_timeout: u64,
}

impl AnyproxyWorkDataStop {
    pub fn new(name: String, is_fast_shutdown: bool, shutdown_timeout: u64) -> Self {
        AnyproxyWorkDataStop {
            name,
            is_fast_shutdown,
            shutdown_timeout,
        }
    }
}

pub struct AnyproxyWorkDataWait {
    pub name: String,
}

impl AnyproxyWorkDataWait {
    pub fn new(name: String) -> Self {
        AnyproxyWorkDataWait { name }
    }
}

pub struct AnyproxyWork {
    executor: ExecutorLocalSpawn,
    config_tx: broadcast::Sender<module::Modules>,
}

impl AnyproxyWork {
    pub fn new(
        executor: ExecutorLocalSpawn,
        config_tx: broadcast::Sender<module::Modules>,
    ) -> Result<AnyproxyWork> {
        Ok(AnyproxyWork {
            executor,
            config_tx,
        })
    }

    pub async fn start(
        &mut self,
        async_context: AsyncThreadContext,
        mut ms: module::Modules,
    ) -> Result<()> {
        log::trace!(target: "main", "anyproxy_work start");

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let data = AnyproxyWorkDataNew::new(self.executor.executors());
        let data = ArcUnsafeAny::new(Box::new(data));
        let mut servers = ms.get_module_servers(data)?;
        #[allow(unused_assignments)]
        let mut mse = None;

        {
            let ms = ms;
            let ms: Result<module::Modules> = tokio::task::spawn_blocking(move || {
                executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                    let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
                    let mut ms = module::Modules::new(Some(ms), true);
                    ms.parse_module_config(&file_name, None)
                        .await
                        .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;
                    Ok(ms)
                })
            })
            .await?;
            let ms = ms.unwrap();

            let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
            ms.init_work_thread(ms_executor.clone())
                .await
                .map_err(|e| anyhow!("err:init_work_thread => e:{}", e))?;

            let data = AnyproxyWorkDataStart::new();
            let data = ArcUnsafeAny::new(Box::new(data));
            for server in servers.iter_mut() {
                if let Err(e) = server
                    .start(ms.clone(), data.clone())
                    .await
                    .map_err(|e| anyhow!("err:start proxy => e:{}", e))
                {
                    return Err(e);
                }

                if log::log_enabled!(target: "ms", log::Level::Debug) {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    log::debug!(target: "ms", "ms session_id:{}, count:{} => server start", ms.session_id(), ms.count());
                }
            }

            mse = Some(ModulesExecutor::new(
                ms.clone(),
                self.executor.clone(),
                ms_executor,
                shutdown_timeout,
            ));
            log::debug!(target: "ms", "ms session_id:{}, count:{} => new mse", ms.session_id(), ms.count());
        }

        async_context.complete();
        let mut shutdown_thread_rx = async_context.shutdown_thread_tx.subscribe();
        let mut config_rx = self.config_tx.subscribe();
        let is_fast_shutdown = loop {
            tokio::select! {
                biased;
                ms = self.config_receiver(&mut config_rx) => {
                    if ms.is_err() {
                        continue;
                    }

                    let ms = ms.unwrap();
                    log::trace!(target: "main", "anyproxy_work reload");
                    let ms: Result<module::Modules> = tokio::task::spawn_blocking(move || {
                        executor_local_spawn::_block_on(1, 512, move |_executor| async move {
                            let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
                            let mut ms = module::Modules::new(Some(ms), true);
                            ms.parse_module_config(&file_name, None)
                                .await
                                .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;
                            Ok(ms)
                            },)
                    })
                    .await?;
                    let  ms = ms.unwrap();

                    let ms_executor = ExecutorLocalSpawn::new(self.executor.executors());
                    ms.init_work_thread(ms_executor.clone()).await.map_err(|e| anyhow!("err:init_work_thread => e:{}", e))?;

                    use crate::config::common_core;
                    let common_conf = common_core::main_conf(&ms).await;
                    let shutdown_timeout = common_conf.shutdown_timeout;

                    let last_ms = if log::log_enabled!(target: "ms", log::Level::Debug) {
                        if mse.is_some() {
                            Some(mse.as_ref().unwrap().ms.clone())
                        } else {
                            None
                        }
                     } else {
                        None
                     };

                    if log::log_enabled!(target: "ms", log::Level::Debug) && last_ms.is_some() {
                        let last_ms = last_ms.as_ref().unwrap();
                            log::debug!(target: "ms", "ms session_id:{}, count:{} => last_ms 111", last_ms.session_id(), last_ms.count());
                    }

                    let data = AnyproxyWorkDataStart::new();
                    let data = ArcUnsafeAny::new(Box::new(data));
                    for server in servers.iter_mut() {
                        if let Err(e) = server.start(ms.clone(), data.clone()).await
                        .map_err(|e| anyhow!("err:proxy.start => e:{}", e)) {
                            log::error!("{}", e);
                            continue;
                        }

                        if log::log_enabled!(target: "ms", log::Level::Debug) {
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            log::debug!(target: "ms", "ms session_id:{}, count:{} => server start", ms.session_id(), ms.count());
                        }
                    }

                    if log::log_enabled!(target: "ms", log::Level::Debug) && last_ms.is_some() {
                        let last_ms = last_ms.as_ref().unwrap();
                            log::debug!(target: "ms", "ms session_id:{}, count:{} => last_ms 222", last_ms.session_id(), last_ms.count());
                    }

                    mse =  Some(ModulesExecutor::new(ms.clone(), self.executor.clone(), ms_executor, shutdown_timeout));
                    log::debug!(target: "ms", "ms session_id:{}, count:{} => new mse", ms.session_id(), ms.count());

                    if log::log_enabled!(target: "ms", log::Level::Debug) && last_ms.is_some() {
                        let last_ms = last_ms.unwrap();
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            log::debug!(target: "ms", "ms session_id:{}, count:{} => last_ms 333", last_ms.session_id(), last_ms.count());
                    }
                }

                ret = shutdown_thread_rx.recv() => {
                    let is_fast_shutdown = if ret.is_err() {
                        true
                    } else {
                        ret.unwrap()
                    };

                    let data = AnyproxyWorkDataSend::new("anyproxy_work send".to_string(),is_fast_shutdown);
                    let data = ArcUnsafeAny::new(Box::new(data));
                    for server in servers.iter_mut() {
                        if let Err(e) = server.send(data.clone()).await {
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

        let data = AnyproxyWorkDataStop::new(
            "anyproxy_work stop server".to_string(),
            is_fast_shutdown,
            shutdown_timeout,
        );
        let data = ArcUnsafeAny::new(Box::new(data));
        for server in servers.iter_mut() {
            if let Err(e) = server.stop(data.clone()).await {
                log::error!("err:proxy.stop => e:{}", e);
            }
        }

        if mse.is_some() {
            drop(mse.unwrap());
        }
        self.executor
            .stop("anyproxy_work stop", is_fast_shutdown, shutdown_timeout)
            .await;
        Ok(())
    }

    pub async fn config_receiver(
        &self,
        config_rx: &mut broadcast::Receiver<module::Modules>,
    ) -> Result<module::Modules> {
        loop {
            let ms = config_rx
                .recv()
                .await
                .map_err(|e| anyhow!("err:config_rx.recv => e:{}", e))?;
            return Ok(ms);
        }
    }
}
