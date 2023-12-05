pub mod connect;
pub mod server;

use any_tunnel2::rt;
use std::future::Future;
//单线程执行器
#[derive(Clone)]
pub struct ExecutorLocal;
impl<F> rt::Executor<F> for ExecutorLocal
where
    F: Future<Output = ()> + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn_local(fut);
    }
}
