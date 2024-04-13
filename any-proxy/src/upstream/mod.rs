pub mod balancer;
pub mod upstream;
pub mod upstream_dynamic_domain_server;
pub mod upstream_heartbeat_server;
pub mod upstream_server;

use crate::config::config_toml::UpstreamDynamicDomain;
use crate::config::config_toml::UpstreamHeartbeat;
use crate::config::upstream_block;
use crate::stream::connect;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct UpstreamHeartbeatData {
    ///vector中第几个域名的索引，自己属于哪个域名
    pub domain_index: usize,
    ///在map中的索引
    pub index: usize,
    pub heartbeat: Option<UpstreamHeartbeat>,
    pub addr: SocketAddr,
    pub connect: Arc<Box<dyn connect::Connect>>,
    pub curr_fail: usize,
    ///心跳失败了，被无效
    pub disable: bool,
    pub shutdown_heartbeat_tx: broadcast::Sender<()>,
    pub is_proxy_protocol_hello: Option<bool>,
    pub total_elapsed: u128,
    pub count_elapsed: usize,
    pub avg_elapsed: u128,
    pub weight: i64,
    pub effective_weight: i64,
    pub current_weight: i64,
}
use crate::config::upstream_core;
use any_base::typ::ShareRw;
use any_base::util::ArcString;

pub struct UpstreamDynamicDomainData {
    ///vector中第几个域名的索引
    pub index: usize,
    pub dynamic_domain: Option<UpstreamDynamicDomain>,
    pub heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>>,
    pub host: String,
    pub addrs: Vec<SocketAddr>,
    pub ups_heartbeat: Option<UpstreamHeartbeat>,
    pub is_weight: bool,
}

//哪个心跳失败删除哪个
//如果host改变了就全部初始化
pub struct UpstreamData {
    ///心跳发生改变了
    pub is_heartbeat_change: bool,
    ///域名发生变化了
    pub is_dynamic_domain_change: bool,
    ///响应时间有变化需要排序了
    pub is_sort_heartbeats_active: bool,
    pub ups_config: Arc<upstream_block::Conf>,
    ///全部域名
    pub ups_dynamic_domains: Vec<ShareRw<UpstreamDynamicDomainData>>,
    ///全部域名对于的ip（包括心跳失败）
    pub ups_heartbeats: Vec<ShareRw<UpstreamHeartbeatData>>,
    ///全部域名对于的活动ip
    pub ups_heartbeats_active: Vec<ShareRw<UpstreamHeartbeatData>>,
    ///全部域名对于的ip
    pub ups_heartbeats_map: HashMap<usize, ShareRw<UpstreamHeartbeatData>>,
    ///map索引，保证唯一
    pub ups_heartbeats_index: usize,
    ///轮询对于的index
    pub round_robin_index: usize,
}

impl UpstreamData {
    pub fn balancer_and_connect(
        &mut self,
        ip: &str,
    ) -> Result<(
        ArcString,
        Option<(Option<bool>, Arc<Box<dyn connect::Connect>>)>,
    )> {
        let plugin_handle_balancer = self.ups_config.plugin_handle_balancer.as_ref().unwrap();
        let connect_info = (plugin_handle_balancer)(ip, self);
        Ok((self.ups_config.balancer.clone(), connect_info))
    }
}
