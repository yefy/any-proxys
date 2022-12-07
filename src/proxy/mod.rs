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
pub mod stream_info;
pub mod stream_stream;
pub mod stream_var;
pub mod tunnel_stream;

use crate::config::config_toml;
use crate::upstream::UpstreamData;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub struct StreamConfigContext {
    pub common: config_toml::CommonConfig,
    pub tcp: config_toml::TcpConfig,
    pub quic: config_toml::QuicConfig,
    pub stream: config_toml::StreamConfig,
    pub rate: config_toml::RateLimit,
    pub tmp_file: config_toml::TmpFile,
    pub fast: config_toml::Fast,
    pub access: Vec<config_toml::AccessConfig>,
    pub access_context: Vec<proxy::AccessContext>,
    pub domain: Option<String>,
    pub proxy_protocol: bool,
    pub ups_data: Rc<RefCell<Option<UpstreamData>>>,
    pub stream_var: Rc<stream_var::StreamVar>,
}

pub enum StreamCache {
    Buffer(Box<StreamCacheBuffer>),
    File(StreamCacheFile),
}

pub struct StreamCacheFile {
    pub size: u64,
}

pub struct StreamCacheBuffer {
    pub data: Vec<u8>,
    pub start: u64,
    pub size: u64,
    pub is_cache: bool,
}

pub struct StreamLimitData {
    pub start_time: Instant,
    pub stream_cache_size: u64,
    pub tmp_file_size: u64,
    pub limit_rate_after: u64,
    pub limit_rate: u64,
    pub max_stream_cache_size: u64,
    pub max_tmp_file_size: u64,
}

pub struct StreamLimit {
    pub tmp_file_size: u64,
    pub limit_rate_after: u64,
    pub limit_rate: u64,
}
impl StreamLimit {
    pub fn new(tmp_file_size: u64, limit_rate_after: u64, limit_rate: u64) -> StreamLimit {
        StreamLimit {
            tmp_file_size,
            limit_rate_after,
            limit_rate,
        }
    }
}
