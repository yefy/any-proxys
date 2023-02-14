use crate::executor_spawn_wait_run::ExecutorSpawnWaitRun;
use crate::rt::{ExecutorTime, SpawnExec};
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use awaitgroup::Worker;
use std::future::Future;
use std::thread;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct ExecutorsSpawn<E: Clone> {
    pub executor: E,
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub wait_group_worker: Worker,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
}

impl<E: Clone> ExecutorsSpawn<E> {
    pub fn _start<S, F>(&self, is_wait: bool, service: S)
    where
        S: FnOnce(ExecutorsSpawn<E>) -> F + 'static + Send,
        F: Future<Output = Result<()>> + 'static + Send,
        E: SpawnExec<F>,
    {
        _start(is_wait, self.clone(), service)
    }

    pub fn sleep(&self, dur: std::time::Duration) -> futures_core::future::BoxFuture<'static, ()>
    where
        E: ExecutorTime,
    {
        self.executor.sleep(dur)
    }

    pub fn error(&self, err: anyhow::Error) {
        self.wait_group_worker.error(err);
    }
}

#[derive(Clone)]
pub struct AsyncContextSpawn<E: Clone> {
    executors: ExecutorsSpawn<E>,
    run_wait_group_worker: Worker,
}

impl<E: Clone> AsyncContextSpawn<E> {
    pub fn new(
        executors: ExecutorsSpawn<E>,
        run_wait_group_worker: Worker,
    ) -> AsyncContextSpawn<E> {
        return AsyncContextSpawn {
            executors,
            run_wait_group_worker,
        };
    }
    pub fn complete(&self) {
        let _ = self.run_wait_group_worker.add();
    }

    pub fn error(&self, err: anyhow::Error) {
        self.executors.error(err);
    }

    pub fn executors(&self) -> ExecutorsSpawn<E> {
        self.executors.clone()
    }
}

#[derive(Clone)]
pub struct ExecutorSpawn<E: Clone> {
    executors: ExecutorsSpawn<E>,
    wait_group: WaitGroup,
}

impl<E: Clone> ExecutorSpawn<E> {
    pub fn executor_spawn_wait_run(&self) -> ExecutorSpawnWaitRun<E> {
        ExecutorSpawnWaitRun::new(self.executors.clone())
    }

    pub fn executors(&self) -> ExecutorsSpawn<E> {
        self.executors.clone()
    }
    pub fn default(executor: E, cpu_affinity: bool, version: i32) -> ExecutorSpawn<E> {
        let thread_id = thread::current().id();
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        let executors = ExecutorsSpawn {
            executor,
            group_version: version,
            thread_id,
            cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        };

        return ExecutorSpawn {
            executors,
            wait_group,
        };
    }

    pub fn new(executors: ExecutorsSpawn<E>) -> ExecutorSpawn<E> {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();
        let executors = ExecutorsSpawn {
            executor: executors.executor.clone(),
            group_version: executors.group_version,
            thread_id: executors.thread_id,
            cpu_affinity: executors.cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        };

        return ExecutorSpawn {
            executors,
            wait_group,
        };
    }

    pub fn _start<S, F>(&mut self, is_wait: bool, service: S)
    where
        S: FnOnce(ExecutorsSpawn<E>) -> F + 'static + Send,
        F: Future<Output = Result<()>> + 'static + Send,
        E: SpawnExec<F>,
    {
        _start(is_wait, self.executors.clone(), service)
    }

    pub fn sleep(&self, dur: std::time::Duration) -> futures_core::future::BoxFuture<'static, ()>
    where
        E: ExecutorTime,
    {
        self.executors.executor.sleep(dur)
    }

    pub fn send(&self, is_fast_shutdown: bool) {
        log::debug!("send version:{}", self.executors.group_version);
        let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
    }

    pub async fn wait(&self) -> Result<()> {
        log::debug!("wait version:{}", self.executors.group_version);
        self.wait_group.wait().await
    }

    pub async fn stop(&self, is_fast_shutdown: bool, shutdown_timeout: u64) {
        log::debug!("stop version:{}", self.executors.group_version);
        if is_fast_shutdown {
            let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
        }
        loop {
            tokio::select! {
                biased;
                ret = self.wait_group.wait() =>  {
                    if let Err(_) = ret {
                        log::error!("err:self.wait_group.wait")
                    }
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(shutdown_timeout)) => {
                    let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
                        log::warn!(
                        "stop timeout: version:{}, wait_group.count:{}",
                        self.executors.group_version,
                            self.wait_group.count(),
                        );
                },
                else => {
                    break;
                }
            }
        }
    }
}

pub fn _start<S, F, E>(is_wait: bool, executors: ExecutorsSpawn<E>, service: S)
where
    S: FnOnce(ExecutorsSpawn<E>) -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
    E: SpawnExec<F>,
{
    let version = executors.group_version;
    log::debug!(
        "start version:{}, worker_threads:{}",
        version,
        executors.wait_group_worker.count(),
    );

    let wait_group_worker_inner = if is_wait {
        Some(executors.wait_group_worker.add())
    } else {
        None
    };

    executors
        .executor
        .clone()
        .spawn(version, wait_group_worker_inner, None, service(executors));
}

pub fn _start_and_free<S, F>(service: S)
where
    S: FnOnce() -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
{
    tokio::spawn(async move {
        let ret: Result<()> = async {
            service()
                .await
                .map_err(|e| anyhow!("err:start spawn service => e:{}", e))?;
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:start spawn => e:{}", e));
    });
}

pub fn _block_on<S, F, E>(thread_num: usize, executor: E, service: S) -> Result<()>
where
    S: FnOnce(ExecutorSpawn<E>) -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
    E: SpawnExec<F>,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(thread_num)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let executor_spawn = ExecutorSpawn::default(executor, false, 0);
            service(executor_spawn)
                .await
                .map_err(|e| anyhow!("err:block_on service => e:{}", e))
        })
        .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
    Ok(())
}
