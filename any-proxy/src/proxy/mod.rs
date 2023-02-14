pub mod domain;
pub mod domain_config;
pub mod domain_server;
pub mod domain_stream;
pub mod heartbeat_stream;
pub mod port;
pub mod port_config;
pub mod port_server;
pub mod port_stream;
pub mod proxy;
pub mod sendfile;
pub mod stream_info;
pub mod stream_start;
pub mod stream_stream;
pub mod stream_var;
pub mod tunnel_stream;

use crate::config::config_toml;
use crate::upstream::UpstreamData;
use crate::util::default_config;
use dynamic_pool::{DynamicPoolItem, DynamicReset};
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

pub struct StreamConfigContext {
    pub common: config_toml::CommonConfig,
    pub tcp: config_toml::TcpConfig,
    pub quic: config_toml::QuicConfig,
    pub stream: config_toml::StreamConfig,
    pub rate: config_toml::RateLimit,
    pub tmp_file: config_toml::TmpFile,
    pub fast_conf: config_toml::FastConf,
    pub access: Vec<config_toml::AccessConfig>,
    pub access_context: Vec<proxy::AccessContext>,
    pub domain: Option<String>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub ups_data: Arc<Mutex<UpstreamData>>,
    pub stream_var: Rc<stream_var::StreamVar>,
}

pub enum StreamCache {
    Buffer(DynamicPoolItem<StreamCacheBuffer>),
    File(StreamCacheFile),
}

pub struct StreamCacheFile {
    pub seek: u64,
    pub size: u64,
}

pub struct StreamCacheBuffer {
    pub datas: Vec<u8>,
    pub start: u64,
    pub size: u64,
    pub is_cache: bool,
    pub seek: u64,
    pub file_fd: i32,
    pub read_size: u64,
    pub min_size: usize,
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
        let min_size = StreamCacheBuffer::min_size();
        StreamCacheBuffer {
            datas: Vec::with_capacity(min_size),
            start: 0,
            size: 0,
            is_cache: false,
            seek: 0,
            file_fd: 0,
            read_size: 0,
            min_size,
        }
    }

    pub fn min_size() -> usize {
        default_config::PAGE_SIZE.load(Ordering::Relaxed) * *default_config::MIN_CACHE_BUFFER_NUM
    }

    pub fn reset(&mut self) {
        unsafe { self.datas.set_len(0) };
        self.start = 0;
        self.size = 0;
        self.is_cache = false;
        self.seek = 0;
        self.file_fd = 0;
        self.read_size = 0;
        //self.min_size
    }

    fn resize(&mut self, data_size: Option<usize>) {
        let data_size = if data_size.is_none() {
            self.min_size
        } else {
            data_size.unwrap()
        };

        if self.datas.len() < data_size {
            self.datas.resize(data_size, 0);
        } else {
            unsafe { self.datas.set_len(data_size) };
        }
        self.size = data_size as u64;
    }
}

pub struct StreamStreamContext {
    pub stream_cache_size: u64,
    pub tmp_file_size: u64,
    pub limit_rate_after: u64,
    pub max_limit_rate: u64,
    pub curr_limit_rate: Arc<AtomicU64>,
    pub max_stream_cache_size: u64,
    pub max_tmp_file_size: u64,
    pub is_tmp_file_io_page: bool,
    pub page_size: usize,
    pub min_merge_cache_buffer_size: u64,
    pub min_cache_buffer_size: usize,
    pub min_read_buffer_size: usize,
    pub min_cache_file_size: usize,
    pub total_read_size: u64,
    pub total_write_size: u64,
}

#[derive(Clone)]
pub struct StreamLimit {
    pub tmp_file_size: u64,
    pub limit_rate_after: u64,
    pub max_limit_rate: u64,
    pub curr_limit_rate: Arc<AtomicU64>,
}
impl StreamLimit {
    pub fn new(tmp_file_size: u64, limit_rate_after: u64, limit_rate: u64) -> StreamLimit {
        StreamLimit {
            tmp_file_size,
            limit_rate_after,
            max_limit_rate: limit_rate,
            curr_limit_rate: Arc::new(AtomicU64::new(limit_rate)),
        }
    }
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
