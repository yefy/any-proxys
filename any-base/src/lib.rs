pub mod anychannel;
pub mod async_wait;
pub mod executor_local_spawn;
pub mod executor_local_spawn_pool;
pub mod executor_local_spawn_pool_wait_run;
pub mod executor_local_spawn_test;
pub mod executor_local_spawn_wait_run;
pub mod executor_spawn;
pub mod executor_spawn_pool;
pub mod executor_spawn_pool_wait_run;
pub mod executor_spawn_test;
pub mod executor_spawn_wait_run;
pub mod executor_test;
pub mod future_wait;
pub mod io;
pub mod io_rb;
pub mod module;
pub mod parking_lot;
pub mod rt;
pub mod std_sync;
pub mod stream;
pub mod stream_buf;
pub mod stream_flow;
pub mod stream_msg_read;
pub mod stream_msg_write;
pub mod thread_pool;
pub mod thread_pool_wait_run;
pub mod thread_spawn;
pub mod thread_spawn_wait_run;
pub mod typ;
pub mod util;

pub const DEFAULT_BUF_SIZE: usize = 8 * 1024;

#[macro_export]
macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}
