use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub trait ExecutorExt: Clone {
    fn execute_ext(&self, fut: Pin<Box<(dyn Future<Output = ()> + Send + 'static)>>);
}

impl<E> ExecutorExt for E
where
    E: Executor<Pin<Box<(dyn Future<Output = ()> + Send + 'static)>>> + Clone,
{
    fn execute_ext(&self, fut: Pin<Box<(dyn Future<Output = ()> + Send + 'static)>>) {
        self.execute(fut);
    }
}

pub trait Executor<Fut> {
    fn execute(&self, fut: Fut);
}

pub(crate) type BoxSendFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Clone)]
pub enum Exec {
    Default,
    Executor(Arc<dyn Executor<BoxSendFuture> + Send + Sync>),
}

impl Exec {
    pub fn new(executor: Arc<dyn Executor<BoxSendFuture> + Send + Sync>) -> Exec {
        Exec::Executor(executor)
    }

    pub fn default() -> Exec {
        Exec::Default
    }

    pub(crate) fn execute<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match *self {
            Exec::Default => {
                tokio::task::spawn(fut);
            }
            Exec::Executor(ref e) => {
                e.execute(Box::pin(fut));
            }
        }
    }
}

impl<F> Executor<F> for Exec
where
    F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, fut: F) {
        self.execute(fut)
    }
}

// use futures_util::task::LocalSpawnExt;
//单线程执行器
// #[derive(Clone)]
// pub struct ExecutorLocal(pub async_executors::TokioCt);
// impl<F> Executor<F> for ExecutorLocal
//     where
//         F: Future<Output = ()> + 'static,
// {
//     fn execute(&self, fut: F) {
//         self.0
//             .spawn_local(fut)
//             .unwrap_or_else(|e| log::error!("{}", e));
//     }
// }
