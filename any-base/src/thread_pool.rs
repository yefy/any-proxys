use super::thread_spawn::ThreadSpawn;
use crate::thread_pool_wait_run::ThreadPoolWaitRun;
use crate::thread_spawn::AsyncThreadContext;
use anyhow::Result;

pub struct ThreadPool {
    worker_threads: usize,
    thread_spawn: ThreadSpawn,
}

impl ThreadPool {
    pub fn new(worker_threads: usize, cpu_affinity: bool, version: i32) -> ThreadPool {
        let thread_spawn = ThreadSpawn::new(cpu_affinity, version);
        return ThreadPool {
            worker_threads,
            thread_spawn,
        };
    }

    pub fn thread_pool_wait_run(&self) -> ThreadPoolWaitRun {
        ThreadPoolWaitRun::new(self.worker_threads, self.thread_spawn.clone())
    }

    pub fn _start<S>(&mut self, service: S)
    where
        S: FnOnce(AsyncThreadContext) -> Result<()> + Clone + Send + 'static,
    {
        for _ in 0..self.worker_threads {
            self.thread_spawn._start::<S>(service.clone());
        }
    }

    pub async fn wait(&mut self) -> Result<()> {
        self.thread_spawn.wait().await
    }

    pub fn send(&self, is_fast_shutdown: bool) {
        self.thread_spawn.send(is_fast_shutdown)
    }

    pub async fn stop(&self, is_fast_shutdown: bool, shutdown_timeout: u64) {
        self.thread_spawn
            .stop(is_fast_shutdown, shutdown_timeout)
            .await
    }
}
//cargo test --color=always thread_pool_test
#[tokio::test]
async fn thread_pool_test() {
    let ret: Result<()> = async {
        let mut thread_pool = ThreadPool::new(2, true, 3);
        thread_pool
            ._start(move |async_context| async move {
                async_context.complete();
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                println!("service run");
                Ok(())
            })
            .await?;
        thread_pool.stop(true).await;
        Ok(())
    }
    .await;
    if let Err(e) = ret {
        log::error!("thread_pool_test:{}", e);
    }
}
