pub mod http_context;
pub mod http_echo_server;
pub mod http_hyper_connector;
pub mod http_hyper_stream;
pub mod http_server;
pub mod http_static_server;
pub mod http_stream;
pub mod stream;
pub mod util;

use any_base::executor_local_spawn::ExecutorsLocal;
use lazy_static::lazy_static;
use std::future::Future;

lazy_static! {
    pub static ref HTTP_HELLO_KEY: String = "http_hello".to_string();
}

//hyper 单线程执行器
#[derive(Clone)]
pub struct HyperExecutorLocal(ExecutorsLocal);
impl<F> hyper::rt::Executor<F> for HyperExecutorLocal
where
    F: Future<Output = ()> + 'static,
{
    fn execute(&self, fut: F) {
        self.0._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
            move |_| async move {
                fut.await;
                Ok(())
            },
        )
    }
}
