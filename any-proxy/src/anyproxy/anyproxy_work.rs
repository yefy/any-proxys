use crate::util::default_config;
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
    pub ms: module::Modules,
}

impl AnyproxyWorkDataNew {
    pub fn new(ms: module::Modules, executors: ExecutorsLocal) -> Self {
        AnyproxyWorkDataNew { ms, executors }
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
        ms: module::Modules,
    ) -> Result<()> {
        log::trace!(target: "main", "anyproxy_work start");
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
        let mut ms = ms.unwrap();

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let data = AnyproxyWorkDataNew::new(ms.clone(), self.executor.executors());
        let data = ArcUnsafeAny::new(Box::new(data));
        let mut servers = ms.get_module_servers(data)?;

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
                    let ms = ms.unwrap();

                    let data = AnyproxyWorkDataStart::new();
                    let data = ArcUnsafeAny::new(Box::new(data));
                    for server in servers.iter_mut() {
                        if let Err(e) = server.start(ms.clone(), data.clone()).await
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
