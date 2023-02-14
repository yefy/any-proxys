use crate::executor_local_spawn_wait_run::ExecutorLocalSpawnWaitRun;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use awaitgroup::Worker;
use futures_util::task::LocalSpawnExt;
use std::future::Future;
use std::thread;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct ExecutorsLocal {
    pub executor: async_executors::TokioCt,
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub wait_group_worker: Worker,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
}

impl ExecutorsLocal {
    pub fn _start<S, F>(&self, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
    {
        _start(self.clone(), service)
    }
    pub fn _start_and_free<S, F>(&self, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
    {
        _start_and_free(self.clone(), service)
    }

    pub fn error(&self, err: anyhow::Error) {
        self.wait_group_worker.error(err);
    }
}

#[derive(Clone)]
pub struct AsyncLocalContext {
    executors: ExecutorsLocal,
    run_wait_group_worker: Worker,
}

impl AsyncLocalContext {
    pub fn new(executors: ExecutorsLocal, run_wait_group_worker: Worker) -> AsyncLocalContext {
        return AsyncLocalContext {
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

    pub fn executors(&self) -> ExecutorsLocal {
        self.executors.clone()
    }
}

#[derive(Clone)]
pub struct ExecutorLocalSpawn {
    executors: ExecutorsLocal,
    wait_group: WaitGroup,
}

impl ExecutorLocalSpawn {
    pub fn executor_local_spawn_wait_run(&self) -> ExecutorLocalSpawnWaitRun {
        ExecutorLocalSpawnWaitRun::new(self.executors.clone())
    }

    pub fn executors(&self) -> ExecutorsLocal {
        self.executors.clone()
    }
    pub fn default(
        executor: async_executors::TokioCt,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorLocalSpawn {
        let thread_id = thread::current().id();
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        let executors = ExecutorsLocal {
            executor,
            group_version: version,
            thread_id,
            cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        };

        return ExecutorLocalSpawn {
            executors,
            wait_group,
        };
    }

    pub fn new(executors: ExecutorsLocal) -> ExecutorLocalSpawn {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();
        let executors = ExecutorsLocal {
            executor: executors.executor.clone(),
            group_version: executors.group_version,
            thread_id: executors.thread_id,
            cpu_affinity: executors.cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        };

        return ExecutorLocalSpawn {
            executors,
            wait_group,
        };
    }

    pub fn _start<S, F>(&mut self, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + 'static,
        F: Future<Output = Result<()>> + 'static,
    {
        _start(self.executors.clone(), service)
    }

    pub fn send(&self, flag: &str, is_fast_shutdown: bool) {
        log::debug!(
            "send version:{}, flag:{}, is_fast_shutdown:{}",
            self.executors.group_version,
            flag,
            is_fast_shutdown
        );
        let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
    }

    pub async fn wait(&self, flag: &str) -> Result<()> {
        log::debug!(
            "wait version:{}, flag:{}",
            self.executors.group_version,
            flag
        );
        self.wait_group.wait().await
    }

    pub async fn stop(&self, flag: &str, is_fast_shutdown: bool, shutdown_timeout: u64) {
        log::debug!(
            "stop version:{}, flag:{}, is_fast_shutdown:{}, \
        shutdown_timeout:{}, wait_group.count:{}",
            self.executors.group_version,
            flag,
            is_fast_shutdown,
            shutdown_timeout,
            self.wait_group.count()
        );

        if is_fast_shutdown {
            let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
        }

        loop {
            tokio::select! {
                biased;
                ret = self.wait_group.wait() =>  {
                    if let Err(_) = ret {
                        log::error!(
                        "err:wait_group.wait: version:{}, flag:{}, wait_group.count:{}",
                        self.executors.group_version,
                        flag,
                            self.wait_group.count(),
                        );
                    }
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(shutdown_timeout)) => {
                    let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
                        log::warn!(
                        "stop timeout: version:{}, flag:{}, wait_group.count:{}",
                        self.executors.group_version,
                        flag,
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

pub fn _start<S, F>(executors: ExecutorsLocal, service: S)
where
    S: FnOnce(ExecutorsLocal) -> F + 'static,
    F: Future<Output = Result<()>> + 'static,
{
    let version = executors.group_version;
    log::debug!(
        "start version:{}, worker_threads:{}",
        version,
        executors.wait_group_worker.count(),
    );

    let wait_group_worker_inner = executors.wait_group_worker.add();

    executors
        .executor
        .clone()
        .spawn_local(async move {
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
        })
        .unwrap_or_else(|e| log::error!("{}", e));
}

pub fn _start_and_free<S, F>(executors: ExecutorsLocal, service: S)
where
    S: FnOnce(ExecutorsLocal) -> F + 'static,
    F: Future<Output = Result<()>> + 'static,
{
    let version = executors.group_version;
    log::debug!(
        "start version:{}, worker_threads:{}",
        version,
        executors.wait_group_worker.count(),
    );

    executors
        .executor
        .clone()
        .spawn_local(async move {
            log::debug!("start executor version:{}", version);
            let ret: Result<()> = async {
                service(executors)
                    .await
                    .map_err(|e| anyhow!("err:start service => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:start spawn_local => e:{}", e));
        })
        .unwrap_or_else(|e| log::error!("{}", e));
}

pub fn _start_and_free2<S, F>(service: S)
where
    S: FnOnce() -> F + 'static + Send,
    F: Future<Output = Result<()>> + 'static + Send,
{
    tokio::task::spawn_local(async move {
        let ret: Result<()> = async {
            service()
                .await
                .map_err(|e| anyhow!("err:start spawn_local service => e:{}", e))?;
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:start spawn_local => e:{}", e));
    });
}

pub fn _block_on<S, F>(worker_threads: usize, service: S) -> Result<()>
where
    S: FnOnce(ExecutorLocalSpawn) -> F + 'static,
    F: Future<Output = Result<()>> + 'static,
{
    let executor = async_executors::TokioCtBuilder::new()
        .build(worker_threads)
        .unwrap();
    executor
        .clone()
        .block_on(async {
            let executor_local_spawn = ExecutorLocalSpawn::default(executor, false, 0);
            service(executor_local_spawn)
                .await
                .map_err(|e| anyhow!("err:block_on service => e:{}", e))
        })
        .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
    Ok(())
}

// pub fn _block_on<S, F, E>(thread_num: usize, executor: E, service: S) -> Result<()>
//     where
//         S: FnOnce(ExecutorSpawn<E>) -> F + 'static + Send,
//         F: Future<Output = Result<()>> + 'static + Send,
//         E: SpawnExec<F>,
// {
//     let rt = tokio::runtime::Builder::new_current_thread()
//         .worker_threads(thread_num)
//         .enable_all()
//         .build()
//         .unwrap();
//     let local = tokio::task::LocalSet::new();
//     local
//         .block_on(&rt, async {
//             let executor_spawn = ExecutorSpawn::default(executor, false, 0);
//             service(executor_spawn)
//                 .await
//                 .map_err(|e| anyhow!("err:block_on service => e:{}", e))
//         })
//         .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
//     Ok(())
// }
