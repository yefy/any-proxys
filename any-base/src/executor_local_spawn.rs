use super::executor_local_spawn_wait_run::ExecutorLocalSpawnWaitRun;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use awaitgroup::Worker;
use std::future::Future;
use std::thread;
use tokio::sync::broadcast;

#[cfg(feature = "anyspawn-count")]
use lazy_static::lazy_static;
use std::pin::Pin;
use std::sync::Arc;
#[cfg(feature = "anyspawn-count")]
lazy_static! {
    pub static ref LOCAL_SPAWN_COUNT_MAP: std::sync::Mutex<std::collections::HashMap<String, i64>> =
        std::sync::Mutex::new(std::collections::HashMap::new());
}

pub trait Runtime: Send + Sync + 'static {
    /// Drive `future` to completion in the background
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);
}

pub struct LocalRuntime;

impl Runtime for LocalRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::task::spawn_local(future);
    }
}

pub struct ThreadRuntime;

impl Runtime for ThreadRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}

pub struct ExecutorsLocalContext {
    pub run_time: Arc<Box<dyn Runtime>>,
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub wait_group_worker: Worker,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
}

impl ExecutorsLocalContext {
    pub fn new(
        run_time: Arc<Box<dyn Runtime>>,
        group_version: i32,
        thread_id: std::thread::ThreadId,
        cpu_affinity: bool,
        wait_group_worker: Worker,
        shutdown_thread_tx: broadcast::Sender<bool>,
    ) -> Self {
        ExecutorsLocalContext {
            run_time,
            group_version,
            thread_id,
            cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        }
    }
}

#[derive(Clone)]
pub struct ExecutorsLocal {
    pub context: Arc<ExecutorsLocalContext>,
}

impl ExecutorsLocal {
    pub fn new(
        run_time: Arc<Box<dyn Runtime>>,
        group_version: i32,
        thread_id: std::thread::ThreadId,
        cpu_affinity: bool,
        wait_group_worker: Worker,
        shutdown_thread_tx: broadcast::Sender<bool>,
    ) -> Self {
        ExecutorsLocal {
            context: Arc::new(ExecutorsLocalContext::new(
                run_time,
                group_version,
                thread_id,
                cpu_affinity,
                wait_group_worker,
                shutdown_thread_tx,
            )),
        }
    }
    pub fn _start<S, F>(&self, #[cfg(feature = "anyspawn-count")] name: Option<String>, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            name,
            true,
            self.clone(),
            service,
        )
    }
    pub fn _start_and_free<S, F>(&self, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            None,
            false,
            self.clone(),
            service,
        )
    }

    pub fn error(&self, err: anyhow::Error) {
        self.context.wait_group_worker.error(err);
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
        run_time: Arc<Box<dyn Runtime>>,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorLocalSpawn {
        let thread_id = thread::current().id();
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        let executors = ExecutorsLocal::new(
            run_time,
            version,
            thread_id,
            cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        );

        return ExecutorLocalSpawn {
            executors,
            wait_group,
        };
    }

    pub fn new(executors: ExecutorsLocal) -> ExecutorLocalSpawn {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();
        let executors = ExecutorsLocal::new(
            executors.context.run_time.clone(),
            executors.context.group_version,
            executors.context.thread_id,
            executors.context.cpu_affinity,
            wait_group_worker,
            shutdown_thread_tx,
        );

        return ExecutorLocalSpawn {
            executors,
            wait_group,
        };
    }

    pub fn _start<S, F>(
        &mut self,
        #[cfg(feature = "anyspawn-count")] name: Option<String>,
        service: S,
    ) where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            name,
            true,
            self.executors.clone(),
            service,
        )
    }

    pub fn _start_and_free<S, F>(&self, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            None,
            false,
            self.executors.clone(),
            service,
        )
    }

    pub fn send(&self, flag: &str, is_fast_shutdown: bool) {
        log::debug!(target: "main",
            "send version:{}, flag:{}, is_fast_shutdown:{}",
            self.executors.context.group_version,
            flag,
            is_fast_shutdown
        );
        let _ = self
            .executors
            .context
            .shutdown_thread_tx
            .send(is_fast_shutdown);
    }

    pub async fn wait(&self, flag: &str) -> Result<()> {
        log::debug!(target: "main",
            "wait version:{}, flag:{}",
            self.executors.context.group_version,
            flag
        );
        self.wait_group.wait().await
    }

    pub async fn wait_group_count() {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            #[cfg(feature = "anyspawn-count")]
            {
                let count_map = LOCAL_SPAWN_COUNT_MAP.lock().unwrap();
                for (k, v) in count_map.iter() {
                    if *v == 0 {
                        continue;
                    }
                    log::info!("wait_group_count: {k} :{v}");
                }
            }
        }
    }

    pub async fn stop(&self, flag: &str, is_fast_shutdown: bool, shutdown_timeout: u64) {
        log::debug!(target: "main",
            "executor_local_spawn stop version:{}, flag:{}, is_fast_shutdown:{}, \
        shutdown_timeout:{}, wait_group.count:{}",
            self.executors.context.group_version,
            flag,
            is_fast_shutdown,
            shutdown_timeout,
            self.wait_group.count()
        );

        if is_fast_shutdown {
            let _ = self
                .executors
                .context
                .shutdown_thread_tx
                .send(is_fast_shutdown);
        }
        let mut num = 0;
        loop {
            tokio::select! {
                biased;
                ret = self.wait_group.wait() =>  {
                    if let Err(_) = ret {
                        log::error!(
                        "err:wait_group.wait: version:{}, flag:{}, wait_group.count:{}",
                        self.executors.context.group_version,
                        flag,
                            self.wait_group.count(),
                        );
                    }
                    break;
                },
                _ = ExecutorLocalSpawn::wait_group_count() =>  {
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(shutdown_timeout)) => {
                    num += 1;
                    if num > 2 {
                        log::error!(
                        "executor_local_spawn stop timeout: version:{}, flag:{}, wait_group.count:{}",
                        self.executors.context.group_version,
                        flag,
                            self.wait_group.count(),
                        );
                        break;
                    }
                    let _ = self.executors.context.shutdown_thread_tx.send(is_fast_shutdown);
                        log::info!(
                        "executor_local_spawn next stop timeout: version:{}, flag:{}, wait_group.count:{}",
                        self.executors.context.group_version,
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

pub fn _start<S, F>(
    #[cfg(feature = "anyspawn-count")] name: Option<String>,
    is_wait: bool,
    executors: ExecutorsLocal,
    service: S,
) where
    S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
    F: Future<Output = Result<()>> + Send + 'static,
{
    let version = executors.context.group_version;
    log::debug!(target: "main",
        "start version:{}, worker_threads:{}",
        version,
        executors.context.wait_group_worker.count(),
    );

    let wait_group_worker_inner = if is_wait {
        Some(executors.context.wait_group_worker.add())
    } else {
        None
    };

    executors
        .context
        .run_time
        .clone()
        .spawn(Box::pin(async move {
            #[cfg(feature = "anyspawn-count")]
            let name_defer = name.clone();
            scopeguard::defer! {
                log::debug!(target: "main", "stop executor version:{}", version);
                if wait_group_worker_inner.is_some() {
                    wait_group_worker_inner.unwrap().done();
                }

                #[cfg(feature = "anyspawn-count")]
                {
                    if name_defer.is_some() {
                        let name_defer = name_defer.unwrap();
                        let mut count_map = LOCAL_SPAWN_COUNT_MAP.lock().unwrap();
                        let count = count_map.get_mut(&name_defer);
                        if count.is_none() {
                            log::error!("err:_start defer name {} nil", name_defer);
                        } else {
                            let count = count.unwrap();
                            *count -= 1;
                            log::debug!(target: "main", "_start defer name {} count:{}", name_defer, count);
                        }
                    }
                }
            }
            log::debug!(target: "main", "start executor version:{}", version);

            #[cfg(feature = "anyspawn-count")]
            {
                if name.is_some() {
                    let name = name.unwrap();
                    let mut count_map = LOCAL_SPAWN_COUNT_MAP.lock().unwrap();
                    let count = count_map.get_mut(&name);
                    if count.is_none() {
                        count_map.insert(name.to_string(), 1);
                        log::debug!(target: "main", "_start name {} count:{}", name, 1);
                    } else {
                        let count = count.unwrap();
                        *count += 1;
                        log::debug!(target: "main", "_start name {} count:{}", name, count);
                    }
                }
            }

            let ret: Result<()> = async {
                service(executors)
                    .await
                    .map_err(|e| anyhow!("err:start service => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:start spawn_local => e:{}", e));
        }));
}

pub fn _start_and_free<S, F>(service: S)
where
    S: FnOnce() -> F + 'static,
    F: Future<Output = Result<()>> + 'static,
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

pub fn _block_on<S, F>(thread_num: usize, worker_threads_blocking: usize, service: S) -> F::Output
where
    S: FnOnce(ExecutorLocalSpawn) -> F + 'static,
    F: Future + 'static,
{
    if thread_num > 1 {
        log::trace!(target: "main", "new_multi_thread thread_num:{}", thread_num);
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(thread_num)
            .max_blocking_threads(worker_threads_blocking)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let executor_spawn =
                    ExecutorLocalSpawn::default(Arc::new(Box::new(ThreadRuntime)), false, 0);
                service(executor_spawn).await
            })
    } else {
        log::trace!(target: "main", "new_current_thread thread_num:{}", thread_num);
        let rt = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(worker_threads_blocking)
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let executor_local_spawn =
                ExecutorLocalSpawn::default(Arc::new(Box::new(LocalRuntime)), false, 0);
            service(executor_local_spawn).await
        })
    }
}
