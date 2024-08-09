extern crate rand;
use crate::stream::connect;
use crate::upstream::balancer::get_connect_data;
use crate::upstream::UpstreamData;
use std::sync::Arc;

pub fn fair(
    _ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, bool, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return get_connect_data(&ups_data.ups_heartbeats, 0);
    }
    return get_connect_data(&ups_data.ups_heartbeats_active, 0);
}
