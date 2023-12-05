extern crate rand;
use crate::stream::connect;
use crate::upstream::UpstreamData;
use std::sync::Arc;

pub fn fair(
    _ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return None;
    }
    let ups_heartbeats = ups_data.ups_heartbeats_active[0].get_mut();
    if ups_heartbeats.disable {
        return None;
    }
    let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
    let connect = ups_heartbeats.connect.clone();
    return Some((is_proxy_protocol_hello, connect));
}
