use serde::{Deserialize, Serialize};
use std::str;

fn default_quic_default() -> bool {
    true
}
fn default_quic_upstream_keepalive() -> usize {
    100
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
    //"h3-29"
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuicConfig {
    #[serde(default = "default_quic_default")]
    pub quic_default: bool,
    #[serde(default = "default_quic_upstream_keepalive")]
    pub quic_upstream_keepalive: usize,
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    #[serde(default = "default_tcp_send_buffer")]
    pub tcp_send_buffer: usize,
    #[serde(default = "default_tcp_recv_buffer")]
    pub tcp_recv_buffer: usize,
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
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
    pub config_log_stdout: bool,
    #[serde(default = "default_max_open_file_limit")]
    pub max_open_file_limit: u64,
}

fn default_tunnel2_worker_thread() -> usize {
    0
}
fn default_tunnel2_max_connect() -> usize {
    1024
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tunnel2Config {
    #[serde(default = "default_tunnel2_worker_thread")]
    pub tunnel2_worker_thread: usize,
    #[serde(default = "default_tunnel2_max_connect")]
    pub tunnel2_max_connect: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigToml {
    pub common: CommonConfig,
    pub tcp: TcpConfig,
    pub quic: QuicConfig,
    pub tunnel2: Tunnel2Config,
    pub stream: StreamConfig,
    pub _port: Option<_PortConfig>,
    pub _domain: Option<_DomainConfig>,
}

fn default_stream_send_timeout() -> u64 {
    10
}
fn default_stream_recv_timeout() -> u64 {
    10
}
fn default_stream_connect_timeout() -> u64 {
    10
}
fn default_stream_cache_size() -> usize {
    8192
}
fn default_stream_work_flow_times() -> bool {
    false
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamConfig {
    #[serde(default = "default_stream_send_timeout")]
    pub stream_send_timeout: u64,
    #[serde(default = "default_stream_recv_timeout")]
    pub stream_recv_timeout: u64,
    #[serde(default = "default_stream_connect_timeout")]
    pub stream_connect_timeout: u64,
    #[serde(default = "default_stream_cache_size")]
    pub stream_cache_size: usize,
    #[serde(default = "default_stream_work_flow_times")]
    pub stream_work_times: bool,
}

fn default_access_log() -> bool {
    true
}

fn default_access_log_file() -> String {
    "./logs/access.log".to_string()
}

fn default_access_format() -> String {
    "[${local_time}] ${local_protocol} -> ${upstream_protocol} ${request_id} ${client_addr} ${remote_addr} ${upstream_addr} ${domain} ${status} ${status_str} ${session_time} ${upstream_connect_time} ${client_bytes_received} ${upstream_bytes_sent} ${upstream_bytes_received} ${client_bytes_sent} [${stream_work_times}]".to_string()
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
    pub ssl_domain: String,
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
    #[serde(rename = "quic")]
    Quic(QuicListenPort),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum DomainListen {
    #[serde(rename = "tcp")]
    Tcp(TcpListen),
    #[serde(rename = "quic")]
    Quic(QuicListenDomain),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassTcp2 {
    pub address: String, //ip:port, domain:port
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassQuic2 {
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "tunnel")]
pub enum ProxyPassTunnel2 {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcp2),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuic2),
    #[serde(rename = "upstream")]
    Upstrem(String),
}

fn default_tunnel_max_connect() -> usize {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassTcp {
    #[serde(default = "default_tunnel_max_connect")]
    pub tunnel_max_connect: usize,
    pub address: String, //ip:port, domain:port
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyPassQuic {
    #[serde(default = "default_tunnel_max_connect")]
    pub tunnel_max_connect: usize,
    pub ssl_domain: String,
    pub address: String, //ip:port, domain:port
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "tunnel")]
pub enum ProxyPassTunnel {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcp),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuic),
    #[serde(rename = "upstream")]
    Upstrem(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum ProxyPass {
    #[serde(rename = "tcp")]
    Tcp(ProxyPassTcp),
    #[serde(rename = "quic")]
    Quic(ProxyPassQuic),
    #[serde(rename = "tunnel")]
    Tunnel(ProxyPassTunnel),
    #[serde(rename = "tunnel2")]
    Tunnel2(ProxyPassTunnel2),
    #[serde(rename = "upstream")]
    Upstrem(String),
}

fn default_access() -> Vec<AccessConfig> {
    vec![AccessConfig {
        access_log: default_access_log(),
        access_log_file: default_access_log_file(),
        access_format: default_access_format(),
        access_log_stdout: default_access_log_stdout(),
    }]
}

fn default_proxy_protocol() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortServerConfig {
    pub common: Option<CommonConfig>,
    pub tcp: Option<TcpConfig>,
    pub quic: Option<QuicConfig>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub access: Option<Vec<AccessConfig>>,
    pub domain: Option<String>,
    pub listen: Vec<PortListen>,
    pub proxy_pass: ProxyPass,
    #[serde(default = "default_proxy_protocol")]
    pub proxy_protocol: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct _PortConfig {
    pub tcp: Option<TcpConfig>,
    pub quic: Option<QuicConfig>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    #[serde(default = "default_access")]
    pub access: Vec<AccessConfig>,
    pub _server: Vec<PortServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainServerConfig {
    pub common: Option<CommonConfig>,
    pub tcp: Option<TcpConfig>,
    pub quic: Option<QuicConfig>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    pub access: Option<Vec<AccessConfig>>,
    pub domain: String,
    pub listen: Option<Vec<DomainListen>>,
    pub proxy_pass: ProxyPass,
    #[serde(default = "default_proxy_protocol")]
    pub proxy_protocol: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct _DomainConfig {
    pub tcp: Option<TcpConfig>,
    pub quic: Option<QuicConfig>,
    pub tunnel2: Option<Tunnel2Config>,
    pub stream: Option<StreamConfig>,
    #[serde(default = "default_access")]
    pub access: Vec<AccessConfig>,
    pub listen: Option<Vec<DomainListen>>,
    pub _server: Vec<DomainServerConfig>,
}
