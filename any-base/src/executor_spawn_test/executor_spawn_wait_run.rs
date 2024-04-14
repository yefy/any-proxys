use super::executor_spawn::{AsyncContextSpawn, ExecutorsSpawn};
use crate::rt::SpawnExec;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use std::future::Future;

#[derive(Clone)]
pub struct ExecutorSpawnWaitRun<E: Clone> {
    worker_threads: usize,
    executors: ExecutorsSpawn<E>,
    run_wait_group: WaitGroup,
}

impl<E: Clone> ExecutorSpawnWaitRun<E> {
    pub fn new(executors: ExecutorsSpawn<E>) -> ExecutorSpawnWaitRun<E> {
        return ExecutorSpawnWaitRun {
            worker_threads: 0,
            executors,
            run_wait_group: WaitGroup::new(),
        };
    }

    pub fn _start<S, F>(&mut self, service: S)
    where
        S: FnOnce(AsyncContextSpawn<E>) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
        E: SpawnExec<F>,
    {
        self.do_start(true, service)
    }

    pub fn _start_and_free<S, F>(&mut self, service: S)
    where
        S: FnOnce(AsyncContextSpawn<E>) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
        E: SpawnExec<F>,
    {
        self.do_start(false, service)
    }

    fn do_start<S, F>(&mut self, is_wait: bool, service: S)
    where
        S: FnOnce(AsyncContextSpawn<E>) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
        E: SpawnExec<F>,
    {
        let index = self.worker_threads;
        self.worker_threads += 1;
        let version = self.executors.group_version;
        log::debug!(target: "main", "start version:{}, worker_threads:{}", version, index,);

        let wait_group_worker_inner = if is_wait {
            Some(self.executors.wait_group_worker.add())
        } else {
            None
        };

        let run_wait_group_worker = self.run_wait_group.worker();
        let err_run_wait_group_worker = self.run_wait_group.worker();
        let executors = self.executors.clone();
        let async_context = AsyncContextSpawn::new(executors, run_wait_group_worker);
        self.executors.executor.spawn(
            version,
            wait_group_worker_inner,
            Some(err_run_wait_group_worker),
            service(async_context),
        );
    }

    pub async fn wait_run(&self) -> Result<()> {
        log::debug!(target: "main",
            "start wait_start version:{}, worker_executor:{}",
            self.executors.group_version,
            self.worker_threads
        );
        self.run_wait_group
            .wait_complete(self.worker_threads)
            .await
            .map_err(|e| anyhow!("err:wait_start => e:{}", e))?;
        log::debug!(target: "main",
            "end wait_start version:{}, worker_executor:{}",
            self.executors.group_version,
            self.worker_threads
        );
        Ok(())
    }
}
