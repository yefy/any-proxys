use crate::thread_spawn::AsyncThreadContext;
use crate::thread_spawn::ThreadSpawn;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;

pub struct ThreadSpawnWaitRun {
    worker_threads: usize,
    thread_spawn: ThreadSpawn,
    run_wait_group: WaitGroup,
}

impl ThreadSpawnWaitRun {
    pub fn new(thread_spawn: ThreadSpawn) -> ThreadSpawnWaitRun {
        ThreadSpawnWaitRun {
            worker_threads: 0,
            thread_spawn,
            run_wait_group: WaitGroup::new(),
        }
    }

    pub fn _start<S>(&mut self, service: S)
    where
        S: FnOnce(AsyncThreadContext) -> Result<()> + Send + 'static,
    {
        self.worker_threads += 1;
        self.thread_spawn
            ._start_or_wait(Some(self.run_wait_group.worker()), service)
    }

    pub async fn wait_run(&mut self) -> Result<()> {
        self.run_wait_group
            .wait_complete(self.worker_threads)
            .await
            .map_err(|e| anyhow!("err:wait_start => e:{}", e))?;
        Ok(())
    }
}
