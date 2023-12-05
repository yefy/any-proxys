use super::executor::ExecutorLocalSpawn;
use super::executor::ExecutorsLocal;
use super::executor::Runtime;
use super::executor_pool_wait_run::ExecutorLocalSpawnPoolWaitRun;
use anyhow::Result;
use std::future::Future;
use std::sync::Arc;

pub struct ExecutorLocalSpawnPool {
    worker_threads: usize,
    executor: ExecutorLocalSpawn,
}

impl ExecutorLocalSpawnPool {
    pub fn executor_local_spawn_pool_wait_run(&self) -> ExecutorLocalSpawnPoolWaitRun {
        ExecutorLocalSpawnPoolWaitRun::new(self.worker_threads, self.executor.executors())
    }

    pub fn default(
        run_time: Arc<Box<dyn Runtime>>,
        worker_threads: usize,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorLocalSpawnPool {
        let executor = ExecutorLocalSpawn::default(run_time, cpu_affinity, version);
        return ExecutorLocalSpawnPool {
            worker_threads,
            executor,
        };
    }
    pub fn new(worker_threads: usize, executors: ExecutorsLocal) -> ExecutorLocalSpawnPool {
        let executor = ExecutorLocalSpawn::new(executors);
        return ExecutorLocalSpawnPool {
            worker_threads,
            executor,
        };
    }

    pub fn _start<S, F>(
        &mut self,
        #[cfg(feature = "anyspawn-count")] name: String,
        service: S,
    ) -> Result<()>
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static + Clone + Send,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        for _ in 0..self.worker_threads {
            self.executor._start::<S, F>(
                #[cfg(feature = "anyspawn-count")]
                name.clone(),
                service.clone(),
            );
        }

        Ok(())
    }

    pub fn _start_and_free<S, F>(&mut self, worker_threads: usize, service: S) -> Result<()>
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static + Clone + Send,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        for _ in 0..worker_threads {
            self.executor._start_and_free::<S, F>(service.clone());
        }

        Ok(())
    }

    pub fn send(&self, flag: &str, is_fast_shutdown: bool) {
        self.executor.send(flag, is_fast_shutdown)
    }

    pub async fn wait(&self, flag: &str) -> Result<()> {
        self.executor.wait(flag).await
    }

    pub async fn stop(&self, flag: &str, is_fast_shutdown: bool, shutdown_timeout: u64) {
        self.executor
            .stop(flag, is_fast_shutdown, shutdown_timeout)
            .await
    }
}
