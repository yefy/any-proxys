use super::executor_spawn::ExecutorSpawn;
use crate::executor_spawn::ExecutorsSpawn;
use crate::executor_spawn_pool_wait_run::ExecutorSpawnPoolWaitRun;
use crate::rt::SpawnExec;
use anyhow::Result;
use std::future::Future;

pub struct ExecutorSpawnPool<E: Clone> {
    worker_threads: usize,
    executor_spawn: ExecutorSpawn<E>,
}

impl<E: Clone> ExecutorSpawnPool<E> {
    pub fn executor_spawn_pool_wait_run(&self) -> ExecutorSpawnPoolWaitRun<E> {
        ExecutorSpawnPoolWaitRun::new(self.worker_threads, self.executor_spawn.executors())
    }

    pub fn default(
        executor: E,
        worker_threads: usize,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorSpawnPool<E> {
        let executor_spawn = ExecutorSpawn::default(executor, cpu_affinity, version);
        return ExecutorSpawnPool {
            worker_threads,
            executor_spawn,
        };
    }
    pub fn new(worker_threads: usize, executors: ExecutorsSpawn<E>) -> ExecutorSpawnPool<E> {
        let executor_spawn = ExecutorSpawn::new(executors);
        return ExecutorSpawnPool {
            worker_threads,
            executor_spawn,
        };
    }

    pub fn _start<S, F>(&mut self, is_wait: bool, service: S) -> Result<()>
    where
        S: FnOnce(ExecutorsSpawn<E>) -> F + 'static + Send + Clone,
        F: Future<Output = Result<()>> + 'static + Send,
        E: SpawnExec<F>,
    {
        for _ in 0..self.worker_threads {
            self.executor_spawn._start::<S, F>(is_wait, service.clone());
        }

        Ok(())
    }

    pub fn send(&self, is_fast_shutdown: bool) {
        self.executor_spawn.send(is_fast_shutdown)
    }

    pub async fn wait(&self) -> Result<()> {
        self.executor_spawn.wait().await
    }

    pub async fn stop(&self, is_fast_shutdown: bool, shutdown_timeout: u64) {
        self.executor_spawn
            .stop(is_fast_shutdown, shutdown_timeout)
            .await
    }
}
