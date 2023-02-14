use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::{Worker, WorkerInner};
use futures_core::future::BoxFuture;
use std::{future::Future, time::Duration};

/// An executor of futures.
pub trait Executor<Fut> {
    /// Place the future into the executor to be run.
    fn execute(
        &self,
        version: i32,
        worker_inner: Option<WorkerInner>,
        err_worker: Option<Worker>,
        fut: Fut,
    );
}

pub trait ExecutorTime {
    fn sleep(&self, dur: Duration) -> BoxFuture<'static, ()>;
}

#[derive(Clone, Copy, Debug)]
pub struct LocalExec;

impl<F> Executor<F> for LocalExec
where
    F: std::future::Future<Output = Result<()>> + 'static, // not requiring `Send`
{
    fn execute(
        &self,
        version: i32,
        worker_inner: Option<WorkerInner>,
        err_worker: Option<Worker>,
        fut: F,
    ) {
        // This will spawn into the currently running `LocalSet`.
        let _ = tokio::task::spawn_local(async move {
            scopeguard::defer! {
                log::debug!("stop spawn_local version:{}", version);
                if worker_inner.is_some(){
                    worker_inner.unwrap().done();
                }
            }

            scopeguard::defer! {
                if err_worker.is_some(){
                    log::debug!("stop spawn_local err version:{}", version);
                    err_worker.unwrap().error(anyhow!("err:spawn_local err_worker"));
                }
            }

            log::debug!("start spawn_local version:{}", version);
            let ret: Result<()> = async {
                fut.await
                    .map_err(|e| anyhow!("err:spawn_local fut => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:spawn_local => e:{}", e));
        });
    }
}

impl ExecutorTime for LocalExec {
    fn sleep(&self, dur: std::time::Duration) -> futures_core::future::BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(dur))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ThreadExec;

impl<F> Executor<F> for ThreadExec
where
    F: std::future::Future<Output = Result<()>> + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(
        &self,
        version: i32,
        worker_inner: Option<WorkerInner>,
        err_worker: Option<Worker>,
        fut: F,
    ) {
        let _ = tokio::task::spawn(async move {
            scopeguard::defer! {
                log::debug!("stop spawn version:{}", version);
                if worker_inner.is_some(){
                    worker_inner.unwrap().done();
                }
            }

            scopeguard::defer! {
                if err_worker.is_some(){
                    log::debug!("stop spawn err version:{}", version);
                    err_worker.unwrap().error(anyhow!("err:spawn err_worker"));
                }
            }
            log::debug!("start spawn version:{}", version);
            let ret: Result<()> = async {
                fut.await.map_err(|e| anyhow!("err:spawn fut => e:{}", e))?;
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:spawn => e:{}", e));
        });
    }
}

impl ExecutorTime for ThreadExec {
    fn sleep(&self, dur: std::time::Duration) -> futures_core::future::BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(dur))
    }
}

pub trait SpawnExec<F>: Clone {
    fn spawn(
        &self,
        version: i32,
        worker_inner: Option<WorkerInner>,
        err_worker: Option<Worker>,
        fut: F,
    );
}

impl<E, F> SpawnExec<F> for E
where
    F: Future<Output = Result<()>> + 'static,
    E: Executor<F> + Clone,
{
    fn spawn(
        &self,
        version: i32,
        worker_inner: Option<WorkerInner>,
        err_worker: Option<Worker>,
        fut: F,
    ) {
        self.execute(version, worker_inner, err_worker, fut)
    }
}

// pub fn _block_on<S, F>(service: S) -> Result<()>
//     where
//         S: FnOnce(ExecutorLocalSpawn) -> F + 'static,
//         F: Future<Output = Result<()>> + 'static,
// {
//     let executor = async_executors::TokioCtBuilder::new().build().unwrap();
//     executor
//         .clone()
//         .block_on(async {
//             let executor_local_spawn = ExecutorLocalSpawn::default(executor, false, 0);
//             service(executor_local_spawn)
//                 .await
//                 .map_err(|e| anyhow!("err:block_on service => e:{}", e))
//         })
//         .map_err(|e| anyhow!("err:_block_on => e:{}", e))?;
//     Ok(())
// }

//
// {
// any_base::executor_spawn::_block_on(
// 1,
// ThreadExec,
// move |mut executor_spawn: ExecutorSpawn<ThreadExec>| async move {
// use any_base::rt::ExecutorTime;
//
// println!("any_base::executor_spawn::_block_on");
// executor_spawn._start(false, move |executors_spawn: ExecutorsSpawn<ThreadExec>| async move {
// executors_spawn.executor.sleep(tokio::time::Duration::from_secs(2)).await;
// println!("111111 thread id:{:?}", std::thread::current().id());
// if true {
// return Err(anyhow!("11111111"));
// }
// Ok(())
// });
// executor_spawn._start(true, move |executors_spawn: ExecutorsSpawn<ThreadExec>| async move {
// executors_spawn.sleep(tokio::time::Duration::from_secs(3)).await;
// println!("2222 thread id:{:?}", std::thread::current().id());
//
// Ok(())
// });
// executor_spawn.sleep(tokio::time::Duration::from_secs(10)).await;
// Ok(())
// },
// )
// .map_err(|e| anyhow!("err:anyproxy block_on => e:{}", e))?;
//
// if true {
// return Ok(());
// }
// }
