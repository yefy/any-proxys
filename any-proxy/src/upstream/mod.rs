pub mod dispatch;
pub mod upstream;
pub mod upstream_config;
pub mod upstream_dynamic_domain_server;
pub mod upstream_heartbeat_server;
pub mod upstream_server;

use super::config::config_toml::ProxyPass;
use crate::config::config_toml;
use crate::config::config_toml::UpstreamDispatch;
use crate::config::config_toml::UpstreamHeartbeat;
use crate::config::config_toml::{UpstreamDynamicDomain, UpstreamServerConfig};
use crate::quic::endpoints;
use crate::stream::connect;
use crate::TunnelClients;
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct UpstreamHeartbeatData {
    ///vector中第几个域名的索引，自己属于哪个域名
    domain_index: usize,
    ///在map中的索引
    index: usize,
    heartbeat: Option<UpstreamHeartbeat>,
    addr: SocketAddr,
    connect: Rc<Box<dyn connect::Connect>>,
    pub curr_fail: usize,
    ///心跳失败了，被无效
    pub disable: bool,
    shutdown_heartbeat_tx: broadcast::Sender<()>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub total_elapsed: u128,
    pub count_elapsed: usize,
    pub avg_elapsed: u128,
    pub weight: i64,
    pub effective_weight: i64,
    pub current_weight: i64,
}

#[derive(Debug, Clone)]
pub struct UpstreamDynamicDomainData {
    ///vector中第几个域名的索引
    index: usize,
    dynamic_domain: Option<UpstreamDynamicDomain>,
    proxy_pass: ProxyPass,
    host: String,
    pub addrs: Vec<SocketAddr>,
    ups_heartbeat: Option<UpstreamHeartbeat>,
    is_weight: bool,
}

#[derive(Clone)]
pub struct UpstreamData {
    ///心跳失败了
    pub is_heartbeat_disable: bool,
    ///域名发生变化了
    pub is_dynamic_domain_change: bool,
    ///响应时间有变化需要排序了
    pub is_sort_heartbeats_active: bool,
    pub ups_config: UpstreamServerConfig,
    ///全部域名
    pub ups_dynamic_domains: Vec<UpstreamDynamicDomainData>,
    ///全部域名对于的ip（包括心跳失败）
    pub ups_heartbeats: Vec<Rc<RefCell<UpstreamHeartbeatData>>>,
    ///全部域名对于的活动ip
    pub ups_heartbeats_active: Vec<Rc<RefCell<UpstreamHeartbeatData>>>,
    ///全部域名对于的ip
    pub ups_heartbeats_map: HashMap<usize, Rc<RefCell<UpstreamHeartbeatData>>>,
    ///map索引，保证唯一
    pub ups_heartbeats_index: usize,
    tcp_config_map: Rc<HashMap<String, config_toml::TcpConfig>>,
    quic_config_map: Rc<HashMap<String, config_toml::QuicConfig>>,
    endpoints_map: Rc<HashMap<String, Arc<endpoints::Endpoints>>>,
    tunnel_clients: TunnelClients,
    ///轮询对于的index
    pub round_robin_index: usize,
}

impl UpstreamData {
    pub fn get_connect(
        &mut self,
        ip: &str,
    ) -> (
        UpstreamDispatch,
        Option<(Option<bool>, Rc<Box<dyn connect::Connect>>)>,
    ) {
        let connect_info = match self.ups_config.dispatch {
            UpstreamDispatch::Weight => dispatch::weight::weight(self),
            UpstreamDispatch::Random => dispatch::random::random(self),
            UpstreamDispatch::RoundRobin => dispatch::round_robin::round_robin(self),
            UpstreamDispatch::IpHash => dispatch::ip_hash::ip_hash(ip, self),
            UpstreamDispatch::IpHashActive => dispatch::ip_hash_active::ip_hash_active(ip, self),
            UpstreamDispatch::Fair => dispatch::fair::fair(self),
        };
        (self.ups_config.dispatch.clone(), connect_info)
    }
}

//哪个心跳失败删除哪个
//如果host改变了就全部初始化
