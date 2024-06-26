use super::executor_wait_run::ExecutorLocalSpawnWaitRun;
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

#[derive(Clone)]
pub struct ExecutorsLocal {
    pub run_time: Arc<Box<dyn Runtime>>,
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub wait_group_worker: Worker,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
}

impl ExecutorsLocal {
    pub fn _start<S, F>(&self, #[cfg(feature = "anyspawn-count")] name: String, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            Some(name),
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
        run_time: Arc<Box<dyn Runtime>>,
        cpu_affinity: bool,
        version: i32,
    ) -> ExecutorLocalSpawn {
        let thread_id = thread::current().id();
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        let executors = ExecutorsLocal {
            run_time,
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
            run_time: executors.run_time.clone(),
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

    pub fn _start<S, F>(&mut self, #[cfg(feature = "anyspawn-count")] name: String, service: S)
    where
        S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
        F: Future<Output = Result<()>> + Send + 'static,
    {
        _start(
            #[cfg(feature = "anyspawn-count")]
            Some(name),
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
            self.executors.group_version,
            flag,
            is_fast_shutdown
        );
        let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown);
    }

    pub async fn wait(&self, flag: &str) -> Result<()> {
        log::debug!(target: "main",
            "wait version:{}, flag:{}",
            self.executors.group_version,
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
                _ = ExecutorLocalSpawn::wait_group_count() =>  {
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

pub fn _start<S, F>(
    #[cfg(feature = "anyspawn-count")] name: Option<String>,
    is_wait: bool,
    executors: ExecutorsLocal,
    service: S,
) where
    S: FnOnce(ExecutorsLocal) -> F + Send + 'static,
    F: Future<Output = Result<()>> + Send + 'static,
{
    let version = executors.group_version;
    log::debug!(target: "main",
        "start version:{}, worker_threads:{}",
        version,
        executors.wait_group_worker.count(),
    );

    let wait_group_worker_inner = if is_wait {
        Some(executors.wait_group_worker.add())
    } else {
        None
    };

    executors.run_time.clone().spawn(Box::pin(async move {
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
                        log::error!("_start name {} nil", name_defer);
                    } else {
                        let count = count.unwrap();
                        *count -= 1;
                        log::info!("_start name {} count:{}", name_defer, count);
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
                    log::info!("-start name {} count:{}", name, 1);
                } else {
                    let count = count.unwrap();
                    *count += 1;
                    log::info!("-start name {} count:{}", name, count);
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

pub fn _block_on<S, F>(worker_threads_blocking: usize, service: S) -> Result<()>
where
    S: FnOnce(ExecutorLocalSpawn) -> F + 'static,
    F: Future<Output = Result<()>> + 'static,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(worker_threads_blocking)
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .block_on(&rt, async {
            let executor_local_spawn =
                ExecutorLocalSpawn::default(Arc::new(Box::new(LocalRuntime)), false, 0);
            service(executor_local_spawn)
                .await
                .map_err(|e| anyhow!("err:block_on service => e:{}", e))
        })
        .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
    Ok(())
}
