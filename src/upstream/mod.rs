pub mod upstream;
pub mod upstream_config;
pub mod upstream_dynamic_domain_server;
pub mod upstream_heartbeat_server;
pub mod upstream_server;

extern crate rand;
use super::config::config_toml::ProxyPass;
use crate::config::config_toml::UpstreamDynamicDomain;
use crate::config::config_toml::UpstreamHeartbeat;
use rand::Rng;
use std::collections::HashMap;

use crate::config::config_toml;
use crate::config::config_toml::UpstreamConfig;
use crate::quic::endpoints;
use crate::stream::connect;
use crate::TunnelClients;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct UpstreamHeartbeatData {
    domain_index: usize,
    index: usize,
    heartbeat: Option<UpstreamHeartbeat>,
    addr: SocketAddr,
    connect: Rc<Box<dyn connect::Connect>>,
    pub curr_fail: usize,
    pub disable: bool,
    shutdown_heartbeat_tx: broadcast::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct UpstreamDynamicDomainData {
    index: usize,
    dynamic_domain: Option<UpstreamDynamicDomain>,
    proxy_pass: ProxyPass,
    host: String,
    pub addrs: Vec<SocketAddr>,
}

#[derive(Clone)]
pub struct UpstreamData {
    pub is_disable_change: bool,
    pub is_change: bool,
    pub ups_config: UpstreamConfig,
    pub ups_dynamic_domains: Vec<UpstreamDynamicDomainData>,
    pub ups_heartbeats: Vec<Rc<RefCell<UpstreamHeartbeatData>>>,
    pub ups_heartbeats_active: Vec<Rc<RefCell<UpstreamHeartbeatData>>>,
    pub ups_heartbeats_map: HashMap<usize, Rc<RefCell<UpstreamHeartbeatData>>>,
    pub ups_heartbeats_index: usize,
    tcp_config_map: Rc<HashMap<String, config_toml::TcpConfig>>,
    quic_config_map: Rc<HashMap<String, config_toml::QuicConfig>>,
    endpoints_map: Rc<HashMap<String, Arc<endpoints::Endpoints>>>,
    tunnel_clients: TunnelClients,
}

impl UpstreamData {
    pub fn connect(&self) -> Option<Rc<Box<dyn connect::Connect>>> {
        let mut rng = rand::thread_rng();
        let index: u8 = rng.gen();
        let mut index = index as usize % self.ups_heartbeats.len();
        let start_index = index;
        loop {
            let ups_heartbeats = self.ups_heartbeats[index].borrow_mut();
            if ups_heartbeats.disable {
                index += 1;
            } else {
                let connect = ups_heartbeats.connect.clone();
                return Some(connect);
            }

            index = index as usize % self.ups_heartbeats.len();
            if index == start_index {
                return None;
            }
        }
    }
}

//哪个心跳失败删除哪个
//如果host改变了就全部初始化
