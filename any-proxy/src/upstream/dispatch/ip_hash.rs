use crate::stream::connect;
use crate::upstream::UpstreamData;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::rc::Rc;

pub fn ip_hash(
    ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, Rc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats.len() <= 0 {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    hasher.write(ip.as_bytes());

    let hash = hasher.finish();
    let index = (hash % ups_data.ups_heartbeats.len() as u64) as usize;

    let ups_heartbeats = ups_data.ups_heartbeats[index].borrow_mut();
    if ups_heartbeats.disable {
        return None;
    }
    let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
    let connect = ups_heartbeats.connect.clone();
    return Some((is_proxy_protocol_hello, connect));
}
