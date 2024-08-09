use crate::thread_spawn_wait_run::ThreadSpawnWaitRun;
use crate::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::{WaitGroup, Worker};
use std::thread;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AsyncThreadContext {
    pub group_version: i32,
    pub thread_id: std::thread::ThreadId,
    pub cpu_affinity: bool,
    pub shutdown_thread_tx: broadcast::Sender<bool>,
    pub run_wait_group_worker: Option<Worker>,
    pub err_wait_group_worker: Worker,
    pub index: usize,
}

impl AsyncThreadContext {
    pub fn new(
        group_version: i32,
        thread_id: std::thread::ThreadId,
        cpu_affinity: bool,
        shutdown_thread_tx: broadcast::Sender<bool>,
        run_wait_group_worker: Option<Worker>,
        err_wait_group_worker: Worker,
        index: usize,
    ) -> AsyncThreadContext {
        return AsyncThreadContext {
            group_version,
            thread_id,
            cpu_affinity,
            shutdown_thread_tx,
            run_wait_group_worker,
            err_wait_group_worker,
            index,
        };
    }
    pub fn complete(&self) {
        if self.run_wait_group_worker.is_some() {
            let run_wait_group_worker = self.run_wait_group_worker.as_ref().unwrap();
            let _ = run_wait_group_worker.add();
        }
    }

    pub fn error(&self, err: anyhow::Error) {
        self.err_wait_group_worker.error(err);
    }
}

#[derive(Clone)]
pub struct ThreadSpawn {
    cpu_affinity: bool,
    version: i32,
    thread_handles: ArcMutex<Vec<std::thread::JoinHandle<()>>>,
    wait_group: WaitGroup,
    wait_group_worker: Worker,
    shutdown_thread_tx: broadcast::Sender<bool>,
}

impl ThreadSpawn {
    pub fn new(cpu_affinity: bool, version: i32) -> ThreadSpawn {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let wait_group = WaitGroup::new();
        let wait_group_worker = wait_group.worker();

        return ThreadSpawn {
            cpu_affinity,
            version,
            thread_handles: ArcMutex::new(Vec::new()),
            wait_group,
            wait_group_worker,
            shutdown_thread_tx,
        };
    }

    pub fn thread_spawn_wait_run(&self) -> ThreadSpawnWaitRun {
        ThreadSpawnWaitRun::new(self.clone())
    }

    pub fn _start<S>(&mut self, service: S)
    where
        S: FnOnce(AsyncThreadContext) -> Result<()> + Send + 'static,
    {
        self._start_or_wait(None, service)
    }

    pub fn _start_or_wait<S>(&mut self, run_wait_group_worker: Option<Worker>, service: S)
    where
        S: FnOnce(AsyncThreadContext) -> Result<()> + Send + 'static,
    {
        let index = self.wait_group_worker.count() as usize;
        let thread_index = index;
        let cpu_affinity = self.cpu_affinity;
        let version = self.version;
        log::info!(
            "start version:{}, worker_threads:{}, cpu_affinity:{}",
            version,
            index,
            cpu_affinity
        );

        let thread_core_ids = core_affinity::get_core_ids();
        let core_id = if thread_core_ids.is_some() {
            let thread_core_ids = thread_core_ids.unwrap();
            let index = index % thread_core_ids.len();
            Some(thread_core_ids[index])
        } else {
            None
        };

        let shutdown_thread_tx = self.shutdown_thread_tx.clone();
        let wait_group_worker_inner = self.wait_group_worker.add();
        let err_wait_group_worker = self.wait_group_worker.worker();
        let thread_handle = thread::spawn(move || {
            let thread_id = thread::current().id();
            scopeguard::defer! {
                log::info!("stop thread version:{} index:{}, worker_threads_id:{:?}", version, index, thread_id);
                wait_group_worker_inner.done();
            }
            log::info!(
                "start thread version:{} index:{}, worker_threads_id:{:?}",
                version,
                index,
                thread_id
            );

            if cpu_affinity && core_id.is_some() {
                let core_id = core_id.unwrap();
                log::info!(
                    "cpu_affinity thread version:{}, index:{} worker_threads_id:{:?} affinity {:?}",
                    version,
                    index,
                    thread_id,
                    core_id
                );
                core_affinity::set_for_current(core_id);
            }

            let err_run_wait_group_worker = run_wait_group_worker.clone();
            scopeguard::defer! {
                if err_run_wait_group_worker.is_some() {
                    err_run_wait_group_worker.unwrap().error(anyhow!("err:thread_spawn"));
                }
            }

            let async_context = AsyncThreadContext::new(
                version,
                thread_id,
                cpu_affinity,
                shutdown_thread_tx,
                run_wait_group_worker,
                err_wait_group_worker,
                thread_index,
            );
            let ret = service(async_context).map_err(|e| anyhow!("err:service_run => e:{}", e));
            if let Err(e) = ret {
                log::error!("err:thread_spawn block_on => e:{}", e);
            }
        });

        self.thread_handles.get_mut().push(thread_handle);
    }

    pub fn send(&self, is_fast_shutdown: bool) {
        log::info!("send version:{}", self.version);
        let _ = self.shutdown_thread_tx.send(is_fast_shutdown);
    }

    pub async fn wait(&self) -> Result<()> {
        self.wait_group.wait().await
    }

    pub async fn stop(&self, is_fast_shutdown: bool, shutdown_timeout: u64) {
        log::debug!(target: "main", "thread_spawn stop version:{}, wait_group:{}", self.version,  self.wait_group.count());
        let _ = self.shutdown_thread_tx.send(is_fast_shutdown);
        let mut num = 0;
        loop {
            tokio::select! {
                biased;
                ret = self.wait_group.wait() =>  {
                    if let Err(_) = ret {
                        log::error!("err:self.async_wait.wait_group.wait")
                    }
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(shutdown_timeout)) => {
                    num += 1;
                    if num > 2 {
                        log::error!(
                        "thread_spawn stop timeout: version:{}, wait_group.count:{}",
                        self.version,
                            self.wait_group.count(),
                        );
                        break;
                    }
                    let _ = self.shutdown_thread_tx.send(is_fast_shutdown);
                        log::info!(
                        "thread_spawn next stop timeout: version:{}, wait_group.count:{}",
                        self.version,
                            self.wait_group.count(),
                        );
                },
                else => {
                    break;
                }
            }
        }

        let thread_handles = unsafe { self.thread_handles.take() };
        if thread_handles.is_some() {
            let thread_handles = thread_handles.unwrap();
            log::info!("thread_spawn stop join version:{}", self.version);
            for handle in thread_handles.into_iter() {
                handle.join().unwrap();
            }
            log::info!("thread_spawn stop done version:{}", self.version);
        }
    }
}
