use crate::stream::connect;
use crate::upstream::balancer::get_connect_data;
use crate::upstream::UpstreamData;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::Arc;

pub fn ip_hash_active(
    ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, bool, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return get_connect_data(&ups_data.ups_heartbeats, 0);
    }

    let mut hasher = DefaultHasher::new();
    hasher.write(ip.as_bytes());

    let hash = hasher.finish();
    let index = (hash % ups_data.ups_heartbeats_active.len() as u64) as usize;

    return get_connect_data(&ups_data.ups_heartbeats_active, index);
}
