pub mod connect;
pub mod server;

use any_tunnel2::rt;
use futures_util::task::LocalSpawnExt;
use std::future::Future;
//单线程执行器
#[derive(Clone)]
pub struct ExecutorLocal(pub async_executors::TokioCt);
impl<F> rt::Executor<F> for ExecutorLocal
where
    F: Future<Output = ()> + 'static,
{
    fn execute(&self, fut: F) {
        self.0
            .spawn_local(fut)
            .unwrap_or_else(|e| log::error!("{}", e));
    }
}
