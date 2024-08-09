use super::upstream_server::UpstreamServer;
use crate::anyproxy::anyproxy_work::{
    AnyproxyWorkDataNew, AnyproxyWorkDataSend, AnyproxyWorkDataStop, AnyproxyWorkDataWait,
};
use any_base::executor_local_spawn::{ExecutorLocalSpawn, ExecutorsLocal};
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;

pub struct Upstream {
    executors: ExecutorsLocal,
    executor: Option<ExecutorLocalSpawn>,
}

impl Upstream {
    pub fn new(value: typ::ArcUnsafeAny) -> Result<Upstream> {
        let value = value.get::<AnyproxyWorkDataNew>();
        Ok(Upstream {
            executors: value.executors.clone(),
            executor: None,
        })
    }
}

#[async_trait]
impl module::Server for Upstream {
    async fn start(&mut self, ms: Modules, _value: ArcUnsafeAny) -> Result<()> {
        //___wait___
        // if ms.is_work_thread() {
        //     return Ok(());
        // }

        let executor = self.executor.take();
        if executor.is_some() {
            let executor = executor.unwrap();
            executor.stop("upstream start", true, 30).await;
        }
        log::debug!(target: "ms", "ms session_id:{}, count:{} => Upstream", ms.session_id(), ms.count());

        let mut executor = ExecutorLocalSpawn::new(self.executors.clone());
        self.executor = Some(executor.clone());
        use crate::config::upstream_core;
        let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
        for (_, ups_data) in upstream_core_conf.ups_data_map.iter() {
            let ups_data = ups_data.clone();
            let ms = ms.clone();
            executor._start(
                #[cfg(feature = "anyspawn-count")]
                Some(format!("{}:{}", file!(), line!())),
                move |executors| async move {
                    let server = UpstreamServer::new(executors, ups_data, ms)
                        .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                    Ok(())
                },
            );
        }
        Ok(())
    }
    async fn stop(&self, value: ArcUnsafeAny) -> Result<()> {
        if self.executor.is_none() {
            return Ok(());
        }
        let executor = self.executor.as_ref().unwrap();
        let value = value.get::<AnyproxyWorkDataStop>();
        scopeguard::defer! {
            log::info!("end upstream stop flag:{}", value.name);
        }
        log::info!("start upstream stop flag:{}", value.name);

        executor
            .stop(&value.name, value.is_fast_shutdown, value.shutdown_timeout)
            .await;
        Ok(())
    }
    async fn send(&self, value: ArcUnsafeAny) -> Result<()> {
        if self.executor.is_none() {
            return Ok(());
        }
        let executor = self.executor.as_ref().unwrap();
        let value = value.get::<AnyproxyWorkDataSend>();
        executor.send(&value.name, value.is_fast_shutdown);
        Ok(())
    }

    async fn wait(&self, value: ArcUnsafeAny) -> Result<()> {
        if self.executor.is_none() {
            return Ok(());
        }
        let executor = self.executor.as_ref().unwrap();
        let value = value.get::<AnyproxyWorkDataWait>();
        executor.wait(&value.name).await
    }
}
