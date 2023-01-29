use crate::executor_spawn::{AsyncContextSpawn, ExecutorsSpawn};
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use std::future::Future;

#[derive(Clone)]
pub struct ExecutorSpawnWaitRun {
    worker_threads: usize,
    executors: ExecutorsSpawn,
    run_wait_group: WaitGroup,
}

impl ExecutorSpawnWaitRun {
    pub fn new(executors: ExecutorsSpawn) -> ExecutorSpawnWaitRun {
        return ExecutorSpawnWaitRun {
            worker_threads: 0,
            executors,
            run_wait_group: WaitGroup::new(),
        };
    }

    pub fn _start<S, F>(&mut self, service: S)
    where
        S: FnOnce(AsyncContextSpawn) -> F + 'static + Send,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        let index = self.worker_threads;
        self.worker_threads += 1;
        let version = self.executors.group_version;
        log::debug!("start version:{}, worker_threads:{}", version, index,);

        let wait_group_worker_inner = self.executors.wait_group_worker.add();
        let run_wait_group_worker = self.run_wait_group.worker();
        let err_run_wait_group_worker = self.run_wait_group.worker();
        let executors = self.executors.clone();
        tokio::spawn(async move {
            scopeguard::defer! {
                log::debug!("stop executor version:{} index:{}", version, index);
                wait_group_worker_inner.done();
            }

            scopeguard::defer! {
                err_run_wait_group_worker.error(anyhow!("err:executor_spawn_wait_run"));
            }

            log::debug!("start executor version:{} index:{}", version, index,);

            let ret: Result<()> = async {
                let async_context = AsyncContextSpawn::new(executors, run_wait_group_worker);
                service(async_context)
                    .await
                    .map_err(|e| anyhow!("err:service_run => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:executor_spawn => e:{}", e));
        });
    }

    pub async fn wait_run(&self) -> Result<()> {
        log::debug!(
            "start wait_start version:{}, worker_executor:{}",
            self.executors.group_version,
            self.worker_threads
        );
        self.run_wait_group
            .wait_complete(self.worker_threads)
            .await
            .map_err(|e| anyhow!("err:wait_start => e:{}", e))?;
        log::debug!(
            "end wait_start version:{}, worker_executor:{}",
            self.executors.group_version,
            self.worker_threads
        );
        Ok(())
    }
}
