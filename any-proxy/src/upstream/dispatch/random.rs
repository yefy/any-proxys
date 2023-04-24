extern crate rand;
use crate::stream::connect;
use crate::upstream::UpstreamData;
use rand::Rng;
use std::sync::Arc;

pub fn random(
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return None;
    }

    let index: usize = rand::thread_rng().gen();
    let mut index = index % ups_data.ups_heartbeats_active.len();
    let start_index = index;
    loop {
        let ups_heartbeats = ups_data.ups_heartbeats_active[index].borrow_mut();
        if ups_heartbeats.disable {
            index += 1;
        } else {
            let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
            let connect = ups_heartbeats.connect.clone();
            return Some((is_proxy_protocol_hello, connect));
        }

        index = index % ups_data.ups_heartbeats.len();
        if index == start_index {
            return None;
        }
    }
}
