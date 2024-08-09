pub mod fair;
pub mod ip_hash;
pub mod ip_hash_active;
pub mod random;
pub mod round_robin;
pub mod weight;

use crate::stream::connect;
use crate::upstream::UpstreamHeartbeatData;
use any_base::typ::ShareRw;
use std::sync::Arc;

pub fn get_connect_data(
    ups_heartbeats: &Vec<ShareRw<UpstreamHeartbeatData>>,
    index: usize,
) -> Option<(Option<bool>, bool, Arc<Box<dyn connect::Connect>>)> {
    if ups_heartbeats.len() <= 0 {
        return None;
    }

    let ups_heartbeat = &ups_heartbeats[index].get_mut();
    let is_proxy_protocol_hello = ups_heartbeat.is_proxy_protocol_hello.clone();
    let connect = ups_heartbeat.connect.clone();
    Some((is_proxy_protocol_hello, ups_heartbeat.disable, connect))
}
