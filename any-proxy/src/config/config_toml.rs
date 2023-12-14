use any_base::util::ArcString;
use serde::{Deserialize, Serialize};
use std::str;

fn default_quic_name() -> String {
    "quic_config_1".to_string()
}
fn default_quic_default() -> bool {
    true
}
fn default_quic_upstream_streams() -> u64 {
    100
}
fn default_quic_send_window() -> u32 {
    8
}
fn default_quic_rtt() -> u32 {
    100
}
fn default_quic_max_stream_bandwidth() -> u32 {
    12500
}
fn default_quic_enable_keylog() -> bool {
    false
}
fn default_quic_protocols() -> String {
    "ALPN".to_string()
}
fn default_quic_datagram_send_buffer_size() -> usize {
    1024 * 1024
}
fn default_quic_recv_buffer_size() -> usize {
    10485760
}
fn default_quic_send_buffer_size() -> usize {
    10485760
}
fn default_quic_send_timeout() -> usize {
    60
}
fn default_quic_recv_timeout() -> usize {
    60
}
fn default_quic_connect_timeout() -> usize {
    10
}
fn default_quic_upstream_ports() -> String {
    "".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuicConfig {
    #[serde(default = "default_quic_name")]
    pub quic_name: String,
    #[serde(default = "default_quic_default")]
    pub quic_default: bool,
    #[serde(default = "default_quic_upstream_ports")]
    pub quic_upstream_ports: String,
    #[serde(default = "default_quic_upstream_streams")]
    pub quic_upstream_streams: u64,
    #[serde(default = "default_quic_send_window")]
    pub quic_send_window: u32,
    #[serde(default = "default_quic_rtt")]
    pub quic_rtt: u32,
    #[serde(default = "default_quic_max_stream_bandwidth")]
    pub quic_max_stream_bandwidth: u32,
    #[serde(default = "default_quic_enable_keylog")]
    pub quic_enable_keylog: bool,
    #[serde(default = "default_quic_protocols")]
    pub quic_protocols: String,
    #[serde(default = "default_quic_datagram_send_buffer_size")]
    pub quic_datagram_send_buffer_size: usize,
    #[serde(default = "default_quic_recv_buffer_size")]
    pub quic_recv_buffer_size: usize,
    #[serde(default = "default_quic_send_buffer_size")]
    pub quic_send_buffer_size: usize,
    #[serde(default = "default_quic_send_timeout")]
    pub quic_send_timeout: usize,
    #[serde(default = "default_quic_recv_timeout")]
    pub quic_recv_timeout: usize,
    #[serde(default = "default_quic_connect_timeout")]
    pub quic_connect_timeout: usize,
}

fn default_tcp_name() -> String {
    "tcp_config_1".to_string()
}
fn default_tcp_send_buffer() -> usize {
    0
}
fn default_tcp_recv_buffer() -> usize {
    0
}
fn default_tcp_nodelay() -> bool {
    true
}
fn default_tcp_send_timeout() -> usize {
    60
}
fn default_tcp_recv_timeout() -> usize {
    60
}
fn default_tcp_connect_timeout() -> usize {
    10
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    #[serde(default = "default_tcp_name")]
    pub tcp_name: String,
    #[serde(default = "default_tcp_send_buffer")]
    pub tcp_send_buffer: usize,
    #[serde(default = "default_tcp_recv_buffer")]
    pub tcp_recv_buffer: usize,
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
    #[serde(default = "default_tcp_send_timeout")]
    pub tcp_send_timeout: usize,
    #[serde(default = "default_tcp_recv_timeout")]
    pub tcp_recv_timeout: usize,
    #[serde(default = "default_tcp_connect_timeout")]
    pub tcp_connect_timeout: usize,
}

fn default_memlock_rlimit_curr() -> u64 {
    128 << 20
}
fn default_memlock_rlimit_max() -> u64 {
    128 << 20
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemlockRlimit {
    #[serde(default = "default_memlock_rlimit_curr")]
    pub curr: u64,
    #[serde(default = "default_memlock_rlimit_max")]
    pub max: u64,
}

fn default_cpu_affinity() -> bool {
    true
}
fn default_reuseport() -> bool {
    true
}
fn default_worker_threads() -> usize {
    0
}
fn default_max_connections() -> i32 {
    0
}
fn default_shutdown_timeout() -> u64 {
    30
}
fn default_config_log_stdout() -> bool {
    false
}
fn default_max_open_file_limit() -> u64 {
    1024003
}
fn default_debug_is_open_ebpf_log() -> bool {
    false
}
fn default_worker_threads_blocking() -> usize {
    5
}
fn default_memlock_rlimit() -> MemlockRlimit {
    MemlockRlimit {
        curr: default_memlock_rlimit_curr(),
        max: default_memlock_rlimit_max(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    #[serde(default = "default_cpu_affinity")]
    pub cpu_affinity: bool,
    #[serde(default = "default_reuseport")]
    pub reuseport: bool,
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    #[serde(default = "default_max_connections")]
    pub max_connections: i32,
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: u64,
    #[serde(default = "default_config_log_stdout")]
    pub debug_is_print_config: bool,
    #[serde(default = "default_max_open_file_limit")]
    pub max_open_file_limit: u64,
    #[serde(default = "default_memlock_rlimit")]
    pub memlock_rlimit: MemlockRlimit,
    #[serde(default = "default_debug_is_open_ebpf_log")]
    pub debug_is_open_ebpf_log: bool,
    #[serde(default = "default_worker_threads_blocking")]
    pub worker_threads_blocking: usize,
}

fn default_tunnel2_worker_thread() -> usize {
    0
}
fn default_tunnel2_max_connect() -> usize {
    100
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tunnel2Config {
    #[serde(default = "default_tunnel2_worker_thread")]
    pub tunnel2_worker_thread: usize,
    #[serde(default = "default_tunnel2_max_connect")]
    pub tunnel2_max_connect: usize,
}

pub fn default_tcp() -> Vec<TcpConfig> {
    vec![TcpConfig {
        tcp_name: default_tcp_name(),
        tcp_send_buffer: default_tcp_send_buffer(),
        tcp_recv_buffer: default_tcp_recv_buffer(),
        tcp_nodelay: default_tcp_nodelay(),
        tcp_send_timeout: default_tcp_send_timeout(),
        tcp_recv_timeout: default_tcp_recv_timeout(),
        tcp_connect_timeout: default_tcp_connect_timeout(),
    }]
}

pub fn default_quic() -> Vec<QuicConfig> {
    vec![QuicConfig {
        quic_name: default_quic_name(),
        quic_default: default_quic_default(),
        quic_upstream_ports: default_quic_upstream_ports(),
        quic_upstream_streams: default_quic_upstream_streams(),
        quic_send_window: default_quic_send_window(),
        quic_rtt: default_quic_rtt(),
        quic_max_stream_bandwidth: default_quic_max_stream_bandwidth(),
        quic_enable_keylog: default_quic_enable_keylog(),
        quic_protocols: default_quic_protocols(),
        quic_datagram_send_buffer_size: default_quic_datagram_send_buffer_size(),
        quic_recv_buffer_size: default_quic_recv_buffer_size(),
        quic_send_buffer_size: default_quic_send_buffer_size(),
        quic_send_timeout: default_quic_send_timeout(),
        quic_recv_timeout: default_quic_recv_timeout(),
        quic_connect_timeout: default_quic_connect_timeout(),
    }]
}

fn default_common() -> CommonConfig {
    CommonConfig {
        cpu_affinity: default_cpu_affinity(),
        reuseport: default_reuseport(),
        worker_threads: default_worker_threads(),
        max_connections: default_max_connections(),
        shutdown_timeout: default_shutdown_timeout(),
        debug_is_print_config: default_config_log_stdout(),
        max_open_file_limit: default_max_open_file_limit(),
        memlock_rlimit: default_memlock_rlimit(),
        debug_is_open_ebpf_log: default_debug_is_open_ebpf_log(),
        worker_threads_blocking: default_worker_threads_blocking(),
    }
}

fn default_tunnel2() -> Tunnel2Config {
    Tunnel2Config {
        tunnel2_worker_thread: default_tunnel2_worker_thread(),
        tunnel2_max_connect: default_tunnel2_max_connect(),
    }
}

fn default_stream() -> StreamConfig {
    StreamConfig {
        stream_cache_size: default_stream_cache_size(),
        debug_is_open_stream_work_times: default_debug_is_open_stream_work_times(),
        debug_print_access_log_time: default_debug_print_access_log_time(),
        debug_print_stream_flow_time: default_debug_print_stream_flow_time(),
        is_tmp_file_io_page: default_is_tmp_file_io_page(),
        stream_so_singer_time: default_stream_so_singer_time(),
    }
}

fn default_rate() -> RateLimit {
    RateLimit {
        download_limit_rate_after: default_download_limit_rate_after(),
        download_limit_rate: default_download_limit_rate(),
        upload_limit_rate_after: default_upload_limit_rate_after(),
        upload_limit_rate: default_upload_limit_rate(),
    }
}

fn default_tmp_file() -> TmpFile {
    TmpFile {
        download_tmp_file_size: default_download_tmp_file_size(),
        upload_tmp_file_size: default_upload_tmp_file_size(),
    }
}

fn default_fast_conf() -> FastConf {
    FastConf {
        is_open_ebpf: default_is_open_ebpf(),
        debug_is_open_print: default_debug_is_open_print(),
        is_open_sendfile: default_is_open_sendfile(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigToml {
    #[serde(default = "default_common")]
    pub common: CommonConfig,
    #[serde(default = "default_tcp")]
    pub tcp: Vec<TcpConfig>,
    #[serde(default = "default_quic")]
    pub quic: Vec<QuicConfig>,
    #[serde(default = "default_tunnel2")]
    pub tunnel2: Tunnel2Config,
    #[serde(default = "default_stream")]
    pub stream: StreamConfig,
    #[serde(default = "default_rate")]
    pub rate: RateLimit,
    #[serde(default = "default_tmp_file")]
    pub tmp_file: TmpFile,
    #[serde(default = "default_fast_conf")]
    pub fast_conf: FastConf,
    pub _port: Option<_PortConfig>,
    pub _domain: Option<_DomainConfig>,
    pub _upstream: Option<_UpstreamConfig>,
}

fn default_is_open_ebpf() -> bool {
    false
}
fn default_debug_is_open_print() -> bool {
    false
}
fn default_is_open_sendfile() -> bool {
    false
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FastConf {
    #[serde(default = "default_is_open_ebpf")]
    pub is_open_ebpf: bool,
    #[serde(default = "default_debug_is_open_print")]
    pub debug_is_open_print: bool,
    #[serde(default = "default_is_open_sendfile")]
    pub is_open_sendfile: bool,
}

fn default_download_limit_rate_after() -> u64 {
    0
}
fn default_download_limit_rate() -> u64 {
    0
}
fn default_upload_limit_rate_after() -> u64 {
    0
}
fn default_upload_limit_rate() -> u64 {
    0
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimit {
    #[serde(default = "default_download_limit_rate_after")]
    pub download_limit_rate_after: u64,
    #[serde(default = "default_download_limit_rate")]
    pub download_limit_rate: u64,
    #[serde(default = "default_upload_limit_rate_after")]
    pub upload_limit_rate_after: u64,
    #[serde(default = "default_upload_limit_rate")]
    pub upload_limit_rate: u64,
}

fn default_download_tmp_file_size() -> u64 {
    0
}
fn default_upload_tmp_file_size() -> u64 {
    0
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TmpFile {
    #[serde(default = "default_download_tmp_file_size")]
    pub download_tmp_file_size: u64,
    #[serde(default = "default_upload_tmp_file_size")]
    pub upload_tmp_file_size: u64,
}

fn default_stream_cache_size() -> usize {
    131072
}
fn default_debug_is_open_stream_work_times() -> bool {
    false
}
fn default_debug_print_access_log_time() -> u64 {
    0
}
fn default_debug_print_stream_flow_time() -> u64 {
    0
}
fn default_is_tmp_file_io_page() -> bool {
    true
}
fn default_stream_so_singer_time() -> usize {
    300
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamConfig {
    #[serde(default = "default_stream_cache_size")]
    pub stream_cache_size: usize,
    #[serde(default = "default_debug_is_open_stream_work_times")]
    pub debug_is_open_stream_work_times: bool,
    #[serde(default = "default_debug_print_access_log_time")]
    pub debug_print_access_log_time: u64,
    #[serde(default = "default_debug_print_stream_flow_time")]
    pub debug_print_stream_flow_time: u64,
    #[serde(default = "default_is_tmp_file_io_page")]
    pub is_tmp_file_io_page: bool,
    #[serde(default = "default_stream_so_singer_time")]
    pub stream_so_singer_time: usize,
}

fn default_access_log() -> bool {
    true
}

fn default_access_log_file() -> String {
    "./logs/access.log".to_string()
}

fn default_access_format() -> String {
    "[${local_time}] ${total_read_size} ${total_write_size} ${write_max_block_time_ms} ${buffer_cache} ${upstream_dispatch} ${is_proxy_protocol_hello} ${is_open_ebpf} ${open_sendfile} ${local_protocol} -> ${upstream_protocol} ${request_id} ${client_addr} ${remote_addr} ${local_addr} ${upstream_addr} ${domain} ${upstream_host} ${status} ${status_str} ${is_timeout_exit} ${session_time} ${upstream_connect_time} ${client_bytes_received} ${upstream_bytes_sent} ${upstream_bytes_received} ${client_bytes_sent} ${upstream_curr_stream_size} ${upstream_max_stream_size} ${upstream_min_stream_cache_size} [${stream_work_times}]".to_string()
}

fn default_access_log_stdout() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessConfig {
    #[serde(default = "default_access_log")]
    pub access_log: bool,
    #[serde(default = "default_access_log_file")]
    pub access_log_file: String,
    #[serde(default = "default_access_format")]
    pub access_format: String,
    #[serde(default = "default_access_log_stdout")]
    pub access_log_stdout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TlsVersion {
    SSLv2,
    SSLv3,
    #[serde(rename = "TLSv1")]
    TLSv1_0,
    #[serde(rename = "TLSv1.1")]
    TLSv1_1,
    #[serde(rename = "TLSv1.2")]
    TLSv1_2,
    #[serde(rename = "TLSv1.3")]
    TLSv1_3,
}

fn default_tls_versions() -> Vec<TlsVersion> {
    vec![
        TlsVersion::TLSv1_1,
        TlsVersion::TLSv1_2,
        TlsVersion::TLSv1_3,
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tls {
    #[serde(default = "default_tls_versions")]
    pub tls_versions: Vec<TlsVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SSL {
    pub ssl_domain: ArcString,
    pub cert: String,
    pub key: String,
    pub tls: Option<Tls>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SSLDomain {
    pub cert: String,
    pub key: String,
    pub tls: Option<Tls>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listen {
    pub address: String,
    pub ssl: Option<SSL>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpListen {
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SslListenPort {
    pub address: String,
    pub ssl: SSL,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SslListenDomain {
    pub address: String,
    pub ssl: SSLDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuicListenPort {
    pub address: String,
    pub ssl: SSL,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuicListenDomain {
    pub address: String,
    pub ssl: SSLDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum PortListen {
    #[serde(rename = "tcp")]
    Tcp(TcpListen),
    #[serde(rename = "ssl")]
    Ssl(SslListenPort),
    #[serde(rename = "quic")]
    Quic(QuicListenPort),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum DomainListen {
    #[serde(rename = "tcp")]
    Tcp(TcpListen),
    #[serde(rename = "ssl")]
    Ssl(SslListenDomain),
    #[serde(rename = "quic")]
    Quic(QuicListenDomain),
}

impl DomainListen {
    pub fn listen_type(&self) -> DomainListenType {
        match &self {
            &DomainListen::Tcp(_) => return DomainListenType::Tcp,
            &DomainListen::Ssl(_) => return DomainListenType::Ssl,
            &DomainListen::Quic(_) => return DomainListenType::Quic,
        }
    }
}

#[derive(Copy, Clone)]
pub enum DomainListenType {
    Tcp = 0,
    Ssl = 1,
    Quic = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassTcpTunnel2 {
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassQuicTunnel2 {
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_quic_name")]
    pub quic: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassSslTunnel2 {
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "tunnel_type")]
pub enum ProxyPassTunnel2 {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcpTunnel2),
    #[serde(rename = "ssl")]
    Ssl(ProxyPassSslTunnel2),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuicTunnel2),
}

fn default_max_stream_size() -> usize {
    5
}
fn default_min_stream_cache_size() -> usize {
    50
}
fn default_channel_size() -> usize {
    64
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tunnel {
    #[serde(default = "default_max_stream_size")]
    pub max_stream_size: usize,
    #[serde(default = "default_min_stream_cache_size")]
    pub min_stream_cache_size: usize,
    #[serde(default = "default_channel_size")]
    pub channel_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassTcp {
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassTcpTunnel {
    pub tunnel: Tunnel,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassSsl {
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassSslTunnel {
    pub tunnel: Tunnel,
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_tcp_name")]
    pub tcp: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassQuic {
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_quic_name")]
    pub quic: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassQuicTunnel {
    pub tunnel: Tunnel,
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
    #[serde(default = "default_quic_name")]
    pub quic: String,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub weight: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "tunnel_type")]
pub enum ProxyPassTunnel {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcpTunnel),
    #[serde(rename = "ssl")]
    Ssl(ProxyPassSslTunnel),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuicTunnel),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassUpstream {
    pub ups_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum ProxyPass {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcp),
    #[serde(rename = "ssl")]
    Ssl(ProxyPassSsl),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuic),
    #[serde(rename = "tunnel")]
    Tunnel(ProxyPassTunnel),
    #[serde(rename = "tunnel2")]
    Tunnel2(ProxyPassTunnel2),
    #[serde(rename = "upstream")]
    Upstream(ProxyPassUpstream),
}

pub fn default_access() -> Vec<AccessConfig> {
    vec![AccessConfig {
        access_log: default_access_log(),
        access_log_file: default_access_log_file(),
        access_format: default_access_format(),
        access_log_stdout: default_access_log_stdout(),
    }]
}

fn default_heartbeat() -> bool {
    false
}

fn default_proxy_pass_upstream() -> String {
    "".to_string()
}
fn default_is_upstream() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortServerConfig {
    pub common: Option<CommonConfig>,
    pub tcp: Option<String>,
    pub quic: Option<String>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub rate: Option<RateLimit>,
    pub tmp_file: Option<TmpFile>,
    pub fast_conf: Option<FastConf>,
    pub access: Option<Vec<AccessConfig>>,
    pub domain: Option<String>,
    pub listen: Vec<PortListen>,
    pub proxy_pass: ProxyPass,
    #[serde(default = "default_proxy_pass_upstream")]
    pub proxy_pass_upstream: String,
    #[serde(default = "default_is_upstream")]
    pub is_upstream: bool,
    pub is_proxy_protocol_hello: Option<bool>,
    #[serde(default = "default_heartbeat")]
    pub heartbeat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct _PortConfig {
    pub tcp: Option<String>,
    pub quic: Option<String>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub rate: Option<RateLimit>,
    pub tmp_file: Option<TmpFile>,
    pub fast_conf: Option<FastConf>,
    #[serde(default = "default_access")]
    pub access: Vec<AccessConfig>,
    pub _server: Vec<PortServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainServerConfig {
    pub common: Option<CommonConfig>,
    pub tcp: Option<String>,
    pub quic: Option<String>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub rate: Option<RateLimit>,
    pub tmp_file: Option<TmpFile>,
    pub fast_conf: Option<FastConf>,
    pub access: Option<Vec<AccessConfig>>,
    pub domain: String,
    pub listen: Option<Vec<DomainListen>>,
    pub proxy_pass: ProxyPass,
    #[serde(default = "default_proxy_pass_upstream")]
    pub proxy_pass_upstream: String,
    #[serde(default = "default_is_upstream")]
    pub is_upstream: bool,
    pub is_proxy_protocol_hello: Option<bool>,
    #[serde(default = "default_heartbeat")]
    pub heartbeat: bool,
    pub server: Option<ServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct _DomainConfig {
    pub tcp: Option<String>,
    pub quic: Option<String>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub rate: Option<RateLimit>,
    pub tmp_file: Option<TmpFile>,
    pub fast_conf: Option<FastConf>,
    #[serde(default = "default_access")]
    pub access: Vec<AccessConfig>,
    pub listen: Option<Vec<DomainListen>>,
    pub _server: Vec<DomainServerConfig>,
}

pub fn default_heartbeat_interval() -> usize {
    10
}

pub fn default_heartbeat_timeout() -> usize {
    10
}

pub fn default_heartbeat_fail() -> usize {
    3
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamHeartbeat {
    #[serde(default = "default_heartbeat_interval")]
    pub interval: usize,
    #[serde(default = "default_heartbeat_timeout")]
    pub timeout: usize,
    #[serde(default = "default_heartbeat_fail")]
    pub fail: usize,
}

pub fn default_dynamic_domain_interval() -> usize {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamDynamicDomain {
    #[serde(default = "default_dynamic_domain_interval")]
    pub interval: usize,
    #[serde(default = "default_heartbeat_timeout")]
    pub timeout: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum UpstreamDispatch {
    #[serde(rename = "weight")]
    Weight,
    #[serde(rename = "round_robin")]
    RoundRobin,
    #[serde(rename = "random")]
    Random,
    #[serde(rename = "ip_hash")]
    IpHash,
    #[serde(rename = "ip_hash_active")]
    IpHashActive,
    #[serde(rename = "fair")]
    Fair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct _UpstreamConfig {
    pub _server: Vec<UpstreamServerConfig>,
}

pub fn default_upstream_dispatch() -> UpstreamDispatch {
    UpstreamDispatch::RoundRobin
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamServerConfig {
    pub name: String,
    #[serde(default = "default_upstream_dispatch")]
    pub dispatch: UpstreamDispatch,
    pub proxy_pass: Vec<ProxyPass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "server")]
pub enum ServerConfig {
    #[serde(rename = "http_server")]
    HttpServer(HttpServer),
    #[serde(rename = "websocket_server")]
    WebsocketServer(WebsocketServer),
}

impl ServerConfig {
    pub fn server_type(&self) -> ServerConfigType {
        match &self {
            &ServerConfig::HttpServer(_) => return ServerConfigType::HttpServer,
            &ServerConfig::WebsocketServer(_) => return ServerConfigType::WebsocketServer,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerConfigType {
    Nil = 0,
    HttpServer = 1,
    WebsocketServer = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServer {
    pub http: HttpServerConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebsocketServer {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "http_server")]
pub enum HttpServerConfig {
    #[serde(rename = "echo_server")]
    EchoServer(HttpServerEchoConfig),
    #[serde(rename = "static_server")]
    StaticServer(HttpServerStaticConfig),
    #[serde(rename = "proxy_server")]
    ProxyServer(HttpServerProxyConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServerEchoConfig {
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServerStaticConfig {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServerProxyConfig {
    pub proxy_pass: HttpServerProxyPassConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum HttpVersion {
    #[serde(rename = "http1.1")]
    Http1_1,
    #[serde(rename = "http2.0")]
    Http2_0,
    #[serde(rename = "auto")]
    Auto,
}

pub fn default_http_server_proxy_pass_config_version() -> HttpVersion {
    HttpVersion::Auto
}
pub fn default_http_server_proxy_pass_config_pool_max_idle_per_host() -> usize {
    100
}
pub fn default_http_server_proxy_pass_config_pool_idle_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServerProxyPassConfig {
    #[serde(default = "default_http_server_proxy_pass_config_version")]
    pub version: HttpVersion,
    #[serde(default = "default_http_server_proxy_pass_config_pool_max_idle_per_host")]
    pub pool_max_idle_per_host: usize,
    #[serde(default = "default_http_server_proxy_pass_config_pool_idle_timeout")]
    pub pool_idle_timeout: u64,
}
