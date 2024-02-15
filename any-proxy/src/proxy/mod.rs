pub mod access_log;
pub mod domain;
pub mod domain_config;
pub mod domain_context;
pub mod domain_server;
pub mod domain_stream;
pub mod heartbeat_stream;
pub mod http_proxy;
pub mod port;
pub mod port_config;
pub mod port_server;
pub mod port_stream;
pub mod proxy;
pub mod sendfile;
pub mod stream_info;
pub mod stream_start;
pub mod stream_stream;
pub mod stream_stream_buf;
pub mod stream_stream_cache;
pub mod stream_stream_memory;
pub mod stream_stream_tmp_file;
pub mod stream_stream_write;
pub mod stream_test;
pub mod stream_var;
pub mod tunnel_stream;
pub mod util;
pub mod websocket_proxy;

use crate::config::common_core;
use crate::config::http_core;
use crate::config::http_core::ConfStream;
use crate::proxy::http_proxy::http_context::HttpContext;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_stream_buf::{
    StreamStreamCacheRead, StreamStreamCacheWrite, StreamStreamCacheWriteDeque,
};
use crate::stream::server::ServerStreamInfo;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::future_wait::FutureWait;
use any_base::io::async_write_msg::BufFile;
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::stream_flow::{StreamFlowRead, StreamFlowWrite};
use any_base::typ;
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcUnsafeAny, Share, ShareRw};
use anyhow::Result;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct StreamConfigContext {
    ms: module::Modules,
    http_confs: Vec<typ::ArcUnsafeAny>,
    server_confs: Vec<typ::ArcUnsafeAny>,
    curr_conf: ArcUnsafeAny,
    common_conf: ArcUnsafeAny,
}

impl StreamConfigContext {
    pub fn new(
        ms: module::Modules,
        http_confs: Vec<typ::ArcUnsafeAny>,
        server_confs: Vec<typ::ArcUnsafeAny>,
        curr_conf: ArcUnsafeAny,
        common_conf: ArcUnsafeAny,
    ) -> Self {
        StreamConfigContext {
            ms,
            http_confs,
            server_confs,
            curr_conf,
            common_conf,
        }
    }
    pub fn ms(&self) -> module::Modules {
        self.ms.clone()
    }
    pub fn common_core_conf(&self) -> &common_core::Conf {
        common_core::curr_conf(&self.common_conf)
    }
    pub fn http_core_conf(&self) -> &http_core::Conf {
        http_core::curr_conf(&self.curr_conf)
    }
    pub fn http_main_confs(&self) -> &Vec<typ::ArcUnsafeAny> {
        &self.http_confs
    }
    pub fn http_server_confs(&self) -> &Vec<typ::ArcUnsafeAny> {
        &self.server_confs
    }
}

#[derive(Clone, Debug)]
pub struct StreamStreamData {
    pub stream_cache_size: i64,
    pub tmp_file_size: i64,
    pub limit_rate_after: i64,
    pub total_read_size: u64,
    pub total_write_size: u64,
    pub stream_nodelay_size: i64,
}

pub struct StreamStreamContext {
    pub cs: Arc<ConfStream>,
    pub curr_limit_rate: Arc<AtomicI64>,
    pub ssd: ArcRwLock<StreamStreamData>,
    pub delay_state: ArcRwLock<DelayState>,
    pub delay_wait: FutureWait,
    pub delay_ok: Arc<AtomicBool>,
    pub delay_version: AtomicUsize,
    pub is_sendfile: bool,
    pub is_client: bool,
    pub close_type: StreamCloseType,
    pub wait_group: awaitgroup::WaitGroup,
    pub worker_inner: awaitgroup::WorkerInner,

    pub read_wait: FutureWait,
    pub other_read_wait: FutureWait,
    pub close_wait: FutureWait,
    pub other_close_wait: FutureWait,
}

const LIMIT_SLEEP_TIME_MILLIS: u64 = 500;
const NORMAL_SLEEP_TIME_MILLIS: u64 = 1000 * 3;
const SENDFILE_EAGAIN_TIME_MILLIS: u64 = 0;

pub fn get_flag(is_client: bool) -> &'static str {
    if is_client {
        "client -> upstream"
    } else {
        "upstream -> client"
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum StreamCloseType {
    Fast,
    Shutdown,
    WaitEmpty,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum DelayState {
    Init,
    Wait,
    Start,
}

pub struct StreamStreamShareContext {
    write_buffer_deque: Option<StreamStreamCacheWriteDeque>,
    write_buffer: Option<StreamStreamCacheWrite>,
    write_err: Option<Result<()>>,
    read_buffer: Option<StreamStreamCacheRead>,
    read_err: Option<Result<()>>,
    caches: LinkedList<StreamStreamCacheWrite>,
    plugins: Vec<Option<ArcUnsafeAny>>,
    is_first_write: bool,
    is_stream_cache: bool,
    #[cfg(unix)]
    is_set_tcp_nopush: bool,
}

pub struct StreamStreamShare {
    sss_ctx: ShareRw<StreamStreamShareContext>,
    ssc: Arc<StreamStreamContext>,
    _scc: ShareRw<StreamConfigContext>,
    stream_info: Share<StreamInfo>,
    r: ArcMutexTokio<StreamFlowRead>,
    w: ArcMutexTokio<StreamFlowWrite>,
    _w_raw_fd: i32,
    is_write_msg: bool,
    is_write_vectored: bool,
}

impl StreamStreamShare {
    pub fn delay_start(&self) {
        if self.ssc.cs.stream_delay_mil_time > 0 {
            let mut delay_state = self.ssc.delay_state.get_mut();
            if *delay_state != DelayState::Start {
                *delay_state = DelayState::Start;
                if self.ssc.delay_wait.is_wait() {
                    self.ssc.delay_wait.waker();
                }
            }
        }
    }

    pub fn delay_is_ok(&self) -> bool {
        self.ssc.delay_ok.load(Ordering::SeqCst)
    }

    pub fn delay_stop(&self) {
        if self.ssc.cs.stream_delay_mil_time > 0 {
            let mut delay_state = self.ssc.delay_state.get_mut();
            if *delay_state != DelayState::Wait {
                self.ssc.delay_version.fetch_add(1, Ordering::SeqCst);
                *delay_state = DelayState::Wait;
            }
        }
    }

    pub fn is_read_empty(&self, sss_ctx: &StreamStreamShareContext) -> bool {
        if sss_ctx.read_buffer.is_none()
            || (sss_ctx.read_buffer.is_some()
                && sss_ctx.read_buffer.as_ref().unwrap().remaining_() == 0)
        {
            return true;
        }
        return false;
    }

    pub fn is_write_empty(&self, sss_ctx: &StreamStreamShareContext) -> bool {
        if sss_ctx.caches.len() <= 0
            && sss_ctx.write_buffer.is_none()
            && sss_ctx.write_buffer_deque.as_ref().unwrap().remaining_() <= 0
        {
            return true;
        }
        return false;
    }

    pub fn is_empty(&self, sss_ctx: &StreamStreamShareContext) -> bool {
        self.is_read_empty(sss_ctx) && self.is_write_empty(sss_ctx)
    }

    pub async fn is_sendfile_close(
        _sss: &StreamStreamShare,
        _stream_status: &StreamStatus,
    ) -> bool {
        let is_sendfile = _sss.ssc.is_sendfile;
        let is_stream_full = if let &StreamStatus::Full(is_sendfile) = _stream_status {
            is_sendfile
        } else {
            false
        };
        if is_stream_full && _sss.stream_info.get().close_num >= 1 && is_sendfile {
            return true;
        }
        return false;
    }

    pub async fn stream_status_sleep(stream_status: &StreamStatus, is_client: bool) {
        match stream_status {
            &StreamStatus::Limit => {
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            &StreamStatus::Full(_) => {}
            &StreamStatus::Ok => {
                log::error!("err:{} -> StreamStatus::Ok", get_flag(is_client));
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            &StreamStatus::WriteEmpty => {
                log::error!(
                    "err:{} -> StreamStatus::DataEmpty from sleep",
                    get_flag(is_client)
                );
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
        };
    }
}

#[derive(Debug, Clone)]
pub enum StreamStatus {
    Limit,
    Full(bool),
    Ok,
    WriteEmpty,
}

#[derive(Clone)]
pub struct StreamTimeout {
    pub timeout_num: Arc<AtomicUsize>,
    pub client_read_timeout_millis: Arc<AtomicI64>,
    pub client_write_timeout_millis: Arc<AtomicI64>,
    pub ups_read_timeout_millis: Arc<AtomicI64>,
    pub ups_write_timeout_millis: Arc<AtomicI64>,
}

impl StreamTimeout {
    pub fn new() -> StreamTimeout {
        StreamTimeout {
            timeout_num: Arc::new(AtomicUsize::new(0)),
            client_read_timeout_millis: Arc::new(AtomicI64::new(0)),
            client_write_timeout_millis: Arc::new(AtomicI64::new(0)),
            ups_read_timeout_millis: Arc::new(AtomicI64::new(0)),
            ups_write_timeout_millis: Arc::new(AtomicI64::new(0)),
        }
    }
}

#[derive(Clone)]
pub struct ServerArg {
    ms: Modules,
    executors: ExecutorsLocal,
    stream_info: Share<StreamInfo>,
    domain_config_listen: Arc<domain_config::DomainConfigListen>,
    server_stream_info: Arc<ServerStreamInfo>,
    http_context: Arc<HttpContext>,
}
