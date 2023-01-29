use crate::executor_spawn_wait_run::ExecutorSpawnWaitRun;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use awaitgroup::Worker;
use std::future::Future;
use std::thread;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct ExecutorsSpawn {
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub wait_group_worker: Worker,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
}

impl ExecutorsSpawn {
    pub fn _start<S, F>(&self, service: S)
    where
        S: FnOnce(ExecutorsSpawn) -> F + 'static + Send,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        _start(self.clone(), service)
    }

    pub fn error(&self, err: anyhow::Error) {
        self.wait_group_worker.error(err);
    }
}

#[derive(Clone)]
pub struct AsyncContextSpawn {
    executors: ExecutorsSpawn,
    run_wait_group_worker: Worker,
}

impl AsyncContextSpawn {
    pub fn new(executors: ExecutorsSpawn, run_wait_group_worker: Worker) -> AsyncContextSpawn {
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

    pub fn executors(&self) -> ExecutorsSpawn {
        self.executors.clone()
    }
}

#[derive(Clone)]
pub struct ExecutorSpawn {
    executors: ExecutorsSpawn,
    wait_group: WaitGroup,
}

impl ExecutorSpawn {
    pub fn executor_spawn_wait_run(&self) -> ExecutorSpawnWaitRun {
        ExecutorSpawnWaitRun::new(self.executors.clone())
    }

    pub fn executors(&self) -> ExecutorsSpawn {
        self.executors.clone()
    }
    pub fn default(cpu_affinity: bool, version: i32) -> ExecutorSpawn {
        let thread_id = thread::current().id();
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        let executors = ExecutorsSpawn {
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

    pub fn new(executors: ExecutorsSpawn) -> ExecutorSpawn {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();
        let executors = ExecutorsSpawn {
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

    pub fn _start<S, F>(&mut self, service: S)
    where
        S: FnOnce(ExecutorsSpawn) -> F + 'static + Send,
        F: Future<Output = Result<()>> + 'static + Send,
    {
        _start(self.executors.clone(), service)
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

        let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
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

pub fn _start<S, F>(executors: ExecutorsSpawn, service: S)
where
    S: FnOnce(ExecutorsSpawn) -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
{
    let version = executors.group_version;
    log::debug!(
        "start version:{}, worker_threads:{}",
        version,
        executors.wait_group_worker.count(),
    );

    let wait_group_worker_inner = executors.wait_group_worker.add();

    tokio::spawn(async move {
        scopeguard::defer! {
            log::debug!("stop executor version:{}", version);
            wait_group_worker_inner.done();
        }
        log::debug!("start executor version:{}", version);

        let ret: Result<()> = async {
            service(executors)
                .await
                .map_err(|e| anyhow!("err:start service => e:{}", e))?;
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:start spawn_local => e:{}", e));
    });
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
                .map_err(|e| anyhow!("err:start service => e:{}", e))?;
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:start spawn_local => e:{}", e));
    });
}

pub fn _block_on<S, F>(thread_num: usize, service: S) -> Result<()>
where
    S: FnOnce(ExecutorSpawn) -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(thread_num)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let executor_spawn = ExecutorSpawn::default(false, 0);
            service(executor_spawn)
                .await
                .map_err(|e| anyhow!("err:block_on service => e:{}", e))
        })
        .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
    Ok(())
}
