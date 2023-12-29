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
pub mod stream_stream_cache;
pub mod stream_stream_memory;
pub mod stream_stream_tmp_file;
pub mod stream_stream_write;
pub mod stream_var;
pub mod tunnel_stream;
pub mod util;
pub mod websocket_proxy;

use crate::config::common_core;
use crate::config::http_core;
use crate::config::http_core::ConfStream;
use crate::proxy::http_proxy::http_context::HttpContext;
#[cfg(unix)]
use crate::proxy::sendfile::SendFile;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::server::ServerStreamInfo;
use crate::util::default_config;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::future_wait::FutureWait;
use any_base::module::module;
use any_base::module::module::Modules;
use any_base::stream_flow::{StreamFlowRead, StreamFlowWrite};
use any_base::typ;
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcUnsafeAny, Share, ShareRw, ValueOption};
use any_base::util::StreamMsg;
use anyhow::Result;
use dynamic_pool::{DynamicPool, DynamicPoolItem, DynamicReset};
use std::collections::LinkedList;
use std::mem::swap;
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

pub enum StreamCache {
    Buffer(DynamicPoolItem<StreamCacheBuffer>),
    File(StreamCacheFile),
}

pub struct StreamCacheFile {
    pub file_fd: i32,
    pub seek: u64,
    pub size: u64,
}

pub struct StreamCacheBuffer {
    pub data: Vec<u8>,
    pub start: u64,
    pub size: u64,
    pub is_cache: bool,
    pub seek: u64,
    pub file_fd: i32,
    pub read_size: u64,
    pub min_size: usize,
    msg: Option<StreamMsg>,
}

impl Default for StreamCacheBuffer {
    fn default() -> Self {
        StreamCacheBuffer::new()
    }
}

impl DynamicReset for StreamCacheBuffer {
    fn reset(&mut self) {
        self.reset()
    }
}

impl StreamCacheBuffer {
    fn new() -> Self {
        let buffer_size = StreamCacheBuffer::buffer_size();
        StreamCacheBuffer {
            data: Vec::with_capacity(buffer_size),
            start: 0,
            size: 0,
            is_cache: false,
            seek: 0,
            file_fd: 0,
            read_size: 0,
            min_size: buffer_size,
            msg: None,
        }
    }

    pub fn push_msg(&mut self, msg: StreamMsg) {
        if self.msg.is_none() {
            self.msg = Some(msg);
        } else {
            self.msg.as_mut().unwrap().push_msg(msg);
        }
    }
    pub fn data_raw(&mut self) -> &mut Vec<u8> {
        return &mut self.data;
    }

    pub fn data_mut(&mut self, start: usize, end: usize) -> &mut [u8] {
        return &mut self.data.as_mut_slice()[start..end];
    }

    pub fn data_or_msg(&mut self, start: usize, end: usize) -> &[u8] {
        if self.msg.is_some() {
            self.msg.as_mut().unwrap().init_and_data(start, end)
        } else {
            &self.data.as_mut_slice()[start..end]
        }
    }

    pub fn to_msg(&mut self, start: usize, end: usize) -> Vec<u8> {
        if self.msg.is_some() {
            return self.msg.take().unwrap().to_vec();
        }
        if start == 0 && end == self.size as usize {
            let mut data = Vec::with_capacity(end);
            swap(&mut self.data, &mut data);
            unsafe { data.set_len(end) };
            return data;
        }
        return self.data.as_slice()[start..end].to_vec();
    }

    pub fn buffer_size() -> usize {
        default_config::PAGE_SIZE.load(Ordering::Relaxed)
            * default_config::MIN_CACHE_BUFFER_NUM.load(Ordering::Relaxed)
    }

    pub fn reset(&mut self) {
        unsafe { self.data.set_len(0) };
        self.start = 0;
        self.size = 0;
        self.is_cache = false;
        self.seek = 0;
        self.file_fd = 0;
        self.read_size = 0;
        //self.min_size
        self.msg = None;
    }

    fn resize(&mut self, data_size: Option<usize>) {
        let data_size = if data_size.is_none() {
            self.min_size
        } else {
            data_size.unwrap()
        };

        if self.data.capacity() < data_size {
            self.data.resize(data_size, 0);
        }

        unsafe { self.data.set_len(data_size) };
        self.size = data_size as u64;
    }

    fn resize_ext(&mut self, data_size: usize) {
        let size = self.min_size + data_size;
        self.resize(Some(size))
    }

    fn resize_curr_size(&mut self) {
        let size = self.size as usize;
        self.resize(Some(size))
    }

    fn set_len(&mut self, size: usize) {
        unsafe { self.data.set_len(size) };
    }
}

#[derive(Clone)]
pub struct StreamStreamData {
    pub stream_cache_size: i64,
    pub tmp_file_size: i64,
    pub limit_rate_after: i64,
    pub total_read_size: u64,
    pub total_write_size: u64,
}

#[derive(Clone)]
pub struct StreamStreamContext {
    pub cs: Arc<ConfStream>,
    pub curr_limit_rate: Arc<AtomicI64>,
    pub ssd: ArcRwLock<StreamStreamData>,
    pub delay_wait: FutureWait,
    pub delay_wait_stop: FutureWait,
    pub delay_ok: Arc<AtomicBool>,
    pub is_delay: Arc<AtomicBool>,
}

const LIMIT_SLEEP_TIME_MILLIS: u64 = 500;
const NORMAL_SLEEP_TIME_MILLIS: u64 = 1000 * 5;
#[cfg(unix)]
const SENDFILE_WRITEABLE_MILLIS: u64 = 100;

pub fn get_flag(is_client: bool) -> &'static str {
    if is_client {
        "client -> upstream"
    } else {
        "upstream -> client"
    }
}

pub struct StreamStreamShare {
    ssc: StreamStreamContext,
    _scc: ShareRw<StreamConfigContext>,
    stream_info: Share<StreamInfo>,
    r: ArcMutexTokio<StreamFlowRead>,
    w: ArcMutexTokio<StreamFlowWrite>,
    #[cfg(unix)]
    sendfile: ArcMutexTokio<SendFile>,
    is_client: bool,
    is_fast_close: bool,
    write_buffer: Option<DynamicPoolItem<StreamCacheBuffer>>,
    write_err: Option<Result<()>>,
    read_buffer: Option<DynamicPoolItem<StreamCacheBuffer>>,
    read_buffer_ret: Option<std::io::Result<usize>>,
    read_err: Option<Result<()>>,
    caches: LinkedList<StreamCache>,
    buffer_pool: ValueOption<DynamicPool<StreamCacheBuffer>>,
    plugins: Vec<Option<ArcUnsafeAny>>,
    is_first_write: bool,
    is_stream_cache: bool,
}

impl StreamStreamShare {
    pub fn delay_start(&self) {
        if self.ssc.is_delay.load(Ordering::SeqCst) {
            return;
        }
        self.ssc.delay_wait.waker();
    }

    pub fn delay_stop(&self) {
        if !self.ssc.is_delay.load(Ordering::SeqCst) {
            return;
        }
        self.ssc.delay_wait_stop.waker();
        self.ssc.delay_ok.store(false, Ordering::SeqCst);
    }

    pub fn is_read_empty(&self) -> bool {
        if self.read_buffer.is_none()
            || (self.read_buffer.is_some()
                && self.read_buffer.as_ref().unwrap().read_size == 0
                && self.read_buffer_ret.is_none())
        {
            return true;
        }
        return false;
    }

    pub fn is_write_empty(&self) -> bool {
        if self.caches.len() <= 0 && self.write_buffer.is_none() {
            return true;
        }
        return false;
    }

    pub fn is_empty(&self) -> bool {
        self.is_read_empty() && self.is_write_empty()
    }

    pub async fn is_sendfile_close(
        _sss: ShareRw<StreamStreamShare>,
        _stream_status: &StreamStatus,
    ) -> bool {
        #[cfg(unix)]
        {
            let sendfile = _sss.get().sendfile.clone();
            let is_sendfile_some = sendfile.get().await.is_some();
            let _sss = _sss.get();
            let is_stream_full = if let &StreamStatus::Full = _stream_status {
                true
            } else {
                false
            };
            if is_stream_full && _sss.stream_info.get().close_num >= 1 && is_sendfile_some {
                return true;
            }
        }
        return false;
    }

    pub async fn stream_status_sleep(stream_status: &StreamStatus, is_client: bool) {
        match stream_status {
            &StreamStatus::Limit => {
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            &StreamStatus::Full => {}
            &StreamStatus::Ok(_) => {
                log::error!("err:{} -> StreamStatus::Ok", get_flag(is_client));
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            &StreamStatus::DataEmpty => {
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
    Full,
    Ok(u64),
    DataEmpty,
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
