use crate::executor_local_spawn::AsyncLocalContext;
use crate::executor_local_spawn::ExecutorsLocal;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use futures_util::task::LocalSpawnExt;
use std::future::Future;

#[derive(Clone)]
pub struct ExecutorLocalSpawnWaitRun {
    worker_threads: usize,
    executors: ExecutorsLocal,
    run_wait_group: WaitGroup,
}

impl ExecutorLocalSpawnWaitRun {
    pub fn new(executors: ExecutorsLocal) -> ExecutorLocalSpawnWaitRun {
        return ExecutorLocalSpawnWaitRun {
            worker_threads: 0,
            executors,
            run_wait_group: WaitGroup::new(),
        };
    }

    pub fn _start<S, F>(&mut self, service: S)
    where
        S: FnOnce(AsyncLocalContext) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
    {
        let index = self.worker_threads;
        self.worker_threads += 1;
        let version = self.executors.group_version;
        log::debug!("start version:{}, worker_threads:{}", version, index,);

        let wait_group_worker_inner = self.executors.wait_group_worker.add();
        let run_wait_group_worker = self.run_wait_group.worker();
        let err_run_wait_group_worker = self.run_wait_group.worker();
        let executors = self.executors.clone();
        executors
            .executor
            .clone()
            .spawn_local(async move {
                scopeguard::defer! {
                    log::debug!("stop executor version:{} index:{}", version, index);
                    wait_group_worker_inner.done();
                }

                scopeguard::defer! {
                    err_run_wait_group_worker.error(anyhow!("err:executor_local_spawn_wait_run"));
                }

                log::debug!("start executor version:{} index:{}", version, index,);

                let ret: Result<()> = async {
                    let async_local_context =
                        AsyncLocalContext::new(executors, run_wait_group_worker);
                    service(async_local_context)
                        .await
                        .map_err(|e| anyhow!("err:service_run => e:{}", e))?;
                    Ok(())
                }
                .await;
                ret.unwrap_or_else(|e| log::error!("err:executor_local_spawn => e:{}", e));
            })
            .unwrap_or_else(|e| log::error!("{}", e));
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
