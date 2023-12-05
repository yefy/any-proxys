use super::upstream_server::UpstreamServer;
use crate::anyproxy::anyproxy_work::{
    AnyproxyWorkDataNew, AnyproxyWorkDataSend, AnyproxyWorkDataStop, AnyproxyWorkDataWait,
};
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;

pub struct Upstream {
    executor: ExecutorLocalSpawn,
}

impl Upstream {
    pub fn new(value: typ::ArcUnsafeAny) -> Result<Upstream> {
        let value = value.get::<AnyproxyWorkDataNew>();
        let executor = ExecutorLocalSpawn::new(value.executors.clone());
        Ok(Upstream { executor })
    }
}

#[async_trait]
impl module::Server for Upstream {
    async fn start(&mut self, ms: Modules, _value: ArcUnsafeAny) -> Result<()> {
        use crate::config::upstream_core;
        let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
        for (_, ups_data) in upstream_core_conf.ups_data_map.iter() {
            let ups_data = ups_data.clone();
            let ms = ms.clone();
            self.executor._start(
                #[cfg(feature = "anyspawn-count")]
                format!("{}:{}", file!(), line!()),
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
        let value = value.get::<AnyproxyWorkDataStop>();
        scopeguard::defer! {
            log::info!("end upstream stop flag:{}", value.name);
        }
        log::info!("start upstream stop flag:{}", value.name);

        self.executor
            .stop(&value.name, value.is_fast_shutdown, value.shutdown_timeout)
            .await;
        Ok(())
    }
    async fn send(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataSend>();
        self.executor.send(&value.name, value.is_fast_shutdown);
        Ok(())
    }

    async fn wait(&self, value: ArcUnsafeAny) -> Result<()> {
        let value = value.get::<AnyproxyWorkDataWait>();
        self.executor.wait(&value.name).await
    }
}
