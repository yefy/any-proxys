use crate::executor_local_spawn::ExecutorLocalSpawn;

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
pub mod file_ext;
pub mod future_wait;
pub mod io;
pub mod io_rb;
pub mod module;
pub mod parking_lot;
pub mod queue;
pub mod rt;
pub mod std_sync;
pub mod stream_buf;
pub mod stream_channel_read;
pub mod stream_channel_write;
pub mod stream_flow;
pub mod stream_nil_read;
pub mod stream_nil_write;
pub mod stream_tokio;
pub mod thread_pool;
pub mod thread_pool_wait_run;
pub mod thread_spawn;
pub mod thread_spawn_wait_run;
pub mod typ;
pub mod typ2;
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

pub struct DropMsExecutor {
    executor: ExecutorLocalSpawn,
    pub ms_executor: ExecutorLocalSpawn,
    pub shutdown_timeout: u64,
}

impl DropMsExecutor {
    pub fn new(
        executor: ExecutorLocalSpawn,
        ms_executor: ExecutorLocalSpawn,
        shutdown_timeout: u64,
    ) -> Self {
        DropMsExecutor {
            executor,
            ms_executor,
            shutdown_timeout,
        }
    }

    pub fn executor(&self) -> ExecutorLocalSpawn {
        self.executor.clone()
    }
    pub fn ms_executor(&self) -> ExecutorLocalSpawn {
        self.ms_executor.clone()
    }
}
