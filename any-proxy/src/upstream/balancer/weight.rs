extern crate rand;
use crate::stream::connect;
use crate::upstream::balancer::get_connect_data;
use crate::upstream::UpstreamData;
use std::sync::Arc;

pub fn weight(
    _ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, bool, Arc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return get_connect_data(&ups_data.ups_heartbeats, 0);
    }

    if ups_data.ups_heartbeats.len() == 1 {
        return get_connect_data(&ups_data.ups_heartbeats_active, 0);
    }

    let mut total = 0;
    let mut index: i32 = -1;
    for (i, ups_heartbeats_active) in ups_data.ups_heartbeats_active.iter().enumerate() {
        let ups_heartbeats_active = &mut *ups_heartbeats_active.get_mut();
        ups_heartbeats_active.current_weight += ups_heartbeats_active.effective_weight;
        total += ups_heartbeats_active.effective_weight;

        if ups_heartbeats_active.effective_weight < ups_heartbeats_active.weight {
            ups_heartbeats_active.effective_weight += 1;
        }

        if index == -1 {
            index = i as i32;
            continue;
        }

        let curr = ups_data.ups_heartbeats_active[index as usize].get();
        if ups_heartbeats_active.current_weight > curr.current_weight {
            index = i as i32;
        }
    }

    let index = index as usize;
    let (is_proxy_protocol_hello, is_disable, connect) =
        get_connect_data(&ups_data.ups_heartbeats_active, index).unwrap();
    if !is_disable {
        ups_data.ups_heartbeats_active[index]
            .get_mut()
            .current_weight -= total;
    }
    return Some((is_proxy_protocol_hello, is_disable, connect));
}
