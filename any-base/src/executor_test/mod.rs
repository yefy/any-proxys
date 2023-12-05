pub mod executor;
pub mod executor_pool;
pub mod executor_pool_wait_run;
pub mod executor_wait_run;

/*
pub trait Executor<Fut> {
    /// Place the future into the executor to be run.
    fn execute(&self, fut: Fut);
}

pub struct LocalExec {

}
impl<F> Executor<F> for LocalExec
    where
    F: Future<Output = ()> + 'static,
{
    fn execute(&self, future: F){
        tokio::task::spawn_local(future);
    }
}



pub struct ThreadExec {

}
impl<F> Executor<F> for ThreadExec
    where
        F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, future: F){
        tokio::task::spawn(future);
    }
}
*/
