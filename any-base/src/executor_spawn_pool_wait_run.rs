use super::executor_spawn_wait_run::ExecutorSpawnWaitRun;
use crate::executor_spawn::AsyncContextSpawn;
use crate::executor_spawn::ExecutorsSpawn;
use anyhow::Result;
use std::future::Future;

pub struct ExecutorSpawnPoolWaitRun {
    worker_threads: usize,
    executor_spawn_wait_run: ExecutorSpawnWaitRun,
}

impl ExecutorSpawnPoolWaitRun {
    pub fn new(worker_threads: usize, executors: ExecutorsSpawn) -> ExecutorSpawnPoolWaitRun {
        let executor_spawn_wait_run = ExecutorSpawnWaitRun::new(executors);
        ExecutorSpawnPoolWaitRun {
            worker_threads,
            executor_spawn_wait_run,
        }
    }

    pub fn _start<S, F>(&mut self, service: S) -> Result<()>
    where
        S: FnOnce(AsyncContextSpawn) -> F + 'static + Send + Clone,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        for _ in 0..self.worker_threads {
            self.executor_spawn_wait_run._start::<S, F>(service.clone());
        }

        Ok(())
    }

    pub async fn wait_run(&self) -> Result<()> {
        self.executor_spawn_wait_run.wait_run().await
    }
}
