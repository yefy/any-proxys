use super::thread_spawn::ThreadSpawn;
use crate::thread_spawn::AsyncThreadContext;
use crate::thread_spawn_wait_run::ThreadSpawnWaitRun;
use anyhow::Result;

pub struct ThreadPoolWaitRun {
    worker_threads: usize,
    thread_spawn_wait_run: ThreadSpawnWaitRun,
}

impl ThreadPoolWaitRun {
    pub fn new(worker_threads: usize, thread_spawn: ThreadSpawn) -> ThreadPoolWaitRun {
        let thread_spawn_wait_run = ThreadSpawnWaitRun::new(thread_spawn);
        return ThreadPoolWaitRun {
            worker_threads,
            thread_spawn_wait_run,
        };
    }

    pub fn _start<S>(&mut self, service: S)
    where
        S: FnOnce(AsyncThreadContext) -> Result<()> + Clone + Send + 'static,
    {
        for _ in 0..self.worker_threads {
            self.thread_spawn_wait_run._start::<S>(service.clone());
        }
    }

    pub async fn wait_run(&mut self) -> Result<()> {
        self.thread_spawn_wait_run.wait_run().await
    }
}
