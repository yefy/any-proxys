use super::executor_local_spawn_wait_run::ExecutorLocalSpawnWaitRun;
use crate::executor_local_spawn::AsyncLocalContext;
use crate::executor_local_spawn::ExecutorsLocal;
use anyhow::Result;
use std::future::Future;

pub struct ExecutorLocalSpawnPoolWaitRun {
    worker_threads: usize,
    executor_local_spawn_wait_run: ExecutorLocalSpawnWaitRun,
}

impl ExecutorLocalSpawnPoolWaitRun {
    pub fn new(worker_threads: usize, executors: ExecutorsLocal) -> ExecutorLocalSpawnPoolWaitRun {
        let executor_local_spawn_wait_run = ExecutorLocalSpawnWaitRun::new(executors);
        ExecutorLocalSpawnPoolWaitRun {
            worker_threads,
            executor_local_spawn_wait_run,
        }
    }

    pub fn _start<S, F>(&mut self, service: S) -> Result<()>
    where
        S: FnOnce(AsyncLocalContext) -> F + 'static + Clone,
        F: Future<Output = Result<()>> + 'static,
    {
        for _ in 0..self.worker_threads {
            self.executor_local_spawn_wait_run
                ._start::<S, F>(service.clone());
        }

        Ok(())
    }

    pub async fn wait_run(&self) -> Result<()> {
        self.executor_local_spawn_wait_run.wait_run().await
    }
}
