extern crate rand;
use crate::stream::connect;
use crate::upstream::balancer::get_connect_data;
use crate::upstream::UpstreamData;
use rand::Rng;
use std::sync::Arc;

pub fn random(
    _ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, bool, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return get_connect_data(&ups_data.ups_heartbeats, 0);
    }

    let index: usize = rand::thread_rng().gen();
    let mut index = index % ups_data.ups_heartbeats_active.len();
    let start_index = index;
    loop {
        let ups_heartbeats = ups_data.ups_heartbeats_active[index].get_mut();
        if ups_heartbeats.disable {
            index += 1;
        } else {
            let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
            let connect = ups_heartbeats.connect.clone();
            return Some((is_proxy_protocol_hello, ups_heartbeats.disable, connect));
        }

        index = index % ups_data.ups_heartbeats.len();
        if index == start_index {
            return get_connect_data(&ups_data.ups_heartbeats, 0);
        }
    }
}
