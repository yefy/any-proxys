use super::executor_local_spawn::ExecutorLocalSpawn;
use crate::executor_local_spawn::ExecutorsLocal;
use crate::executor_local_spawn_pool_wait_run::ExecutorLocalSpawnPoolWaitRun;
use anyhow::Result;
use std::future::Future;

pub struct ExecutorLocalSpawnPool {
    worker_threads: usize,
    executor_local_spawn: ExecutorLocalSpawn,
}

impl ExecutorLocalSpawnPool {
    pub fn executor_local_spawn_pool_wait_run(&self) -> ExecutorLocalSpawnPoolWaitRun {
        ExecutorLocalSpawnPoolWaitRun::new(
            self.worker_threads,
            self.executor_local_spawn.executors(),
        )
    }

    pub fn default(
        worker_threads: usize,
        executor: async_executors::TokioCt,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorLocalSpawnPool {
        let executor_local_spawn = ExecutorLocalSpawn::default(executor, cpu_affinity, version);
        return ExecutorLocalSpawnPool {
            worker_threads,
            executor_local_spawn,
        };
    }
    pub fn new(worker_threads: usize, executors: ExecutorsLocal) -> ExecutorLocalSpawnPool {
        let executor_local_spawn = ExecutorLocalSpawn::new(executors);
        return ExecutorLocalSpawnPool {
            worker_threads,
            executor_local_spawn,
        };
    }

    pub fn _start<S, F>(
        &mut self,
        #[cfg(feature = "anyspawn-count")] name: String,
        service: S,
    ) -> Result<()>
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static + Clone,
        F: Future<Output = Result<()>> + 'static,
    {
        for _ in 0..self.worker_threads {
            self.executor_local_spawn._start::<S, F>(
                #[cfg(feature = "anyspawn-count")]
                name.clone(),
                service.clone(),
            );
        }

        Ok(())
    }

    pub fn send(&self, flag: &str, is_fast_shutdown: bool) {
        self.executor_local_spawn.send(flag, is_fast_shutdown)
    }

    pub async fn wait(&self, flag: &str) -> Result<()> {
        self.executor_local_spawn.wait(flag).await
    }

    pub async fn stop(&self, flag: &str, is_fast_shutdown: bool, shutdown_timeout: u64) {
        self.executor_local_spawn
            .stop(flag, is_fast_shutdown, shutdown_timeout)
            .await
    }
}
