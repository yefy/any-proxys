use super::executor_spawn::AsyncContextSpawn;
use super::executor_spawn::ExecutorsSpawn;
use super::executor_spawn_wait_run::ExecutorSpawnWaitRun;
use crate::rt::SpawnExec;
use anyhow::Result;
use std::future::Future;

pub struct ExecutorSpawnPoolWaitRun<E: Clone> {
    worker_threads: usize,
    executor_spawn_wait_run: ExecutorSpawnWaitRun<E>,
}

impl<E: Clone> ExecutorSpawnPoolWaitRun<E> {
    pub fn new(worker_threads: usize, executors: ExecutorsSpawn<E>) -> ExecutorSpawnPoolWaitRun<E> {
        let executor_spawn_wait_run = ExecutorSpawnWaitRun::new(executors);
        ExecutorSpawnPoolWaitRun {
            worker_threads,
            executor_spawn_wait_run,
        }
    }

    pub fn _start<S, F>(&mut self, service: S) -> Result<()>
    where
        S: FnOnce(AsyncContextSpawn<E>) -> F + 'static + Clone,
        F: Future<Output = Result<()>> + 'static,
        E: SpawnExec<F>,
    {
        for _ in 0..self.worker_threads {
            self.executor_spawn_wait_run._start::<S, F>(service.clone());
        }

        Ok(())
    }

    pub fn _start_and_free<S, F>(&mut self, worker_threads: usize, service: S) -> Result<()>
    where
        S: FnOnce(AsyncContextSpawn<E>) -> F + 'static + Clone,
        F: Future<Output = Result<()>> + 'static,
        E: SpawnExec<F>,
    {
        for _ in 0..worker_threads {
            self.executor_spawn_wait_run
                ._start_and_free::<S, F>(service.clone());
        }

        Ok(())
    }

    pub async fn wait_run(&self) -> Result<()> {
        self.executor_spawn_wait_run.wait_run().await
    }
}
