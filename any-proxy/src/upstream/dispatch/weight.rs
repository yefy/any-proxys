extern crate rand;
use crate::stream::connect;
use crate::upstream::UpstreamData;
use std::rc::Rc;

pub fn weight(
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, Rc<Box<dyn connect::Connect>>)> {
    if ups_data.ups_heartbeats_active.len() <= 0 {
        return None;
    }

    if ups_data.ups_heartbeats.len() == 1 {
        let ups_heartbeats = ups_data.ups_heartbeats_active[0].borrow_mut();
        if ups_heartbeats.disable {
            return None;
        }
        let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
        let connect = ups_heartbeats.connect.clone();
        return Some((is_proxy_protocol_hello, connect));
    }

    let mut total = 0;
    let mut index: i32 = -1;
    for (i, ups_heartbeats_active) in ups_data.ups_heartbeats_active.iter().enumerate() {
        let ups_heartbeats_active = &mut *ups_heartbeats_active.borrow_mut();
        ups_heartbeats_active.current_weight += ups_heartbeats_active.effective_weight;
        total += ups_heartbeats_active.effective_weight;

        if ups_heartbeats_active.effective_weight < ups_heartbeats_active.weight {
            ups_heartbeats_active.effective_weight += 1;
        }

        if index == -1 {
            index = i as i32;
            continue;
        }

        let curr = ups_data.ups_heartbeats_active[index as usize].borrow();
        if ups_heartbeats_active.current_weight > curr.current_weight {
            index = i as i32;
        }
    }

    let ups_heartbeats = &mut *ups_data.ups_heartbeats_active[index as usize].borrow_mut();
    if ups_heartbeats.disable {
        return None;
    }
    ups_heartbeats.current_weight -= total;
    let is_proxy_protocol_hello = ups_heartbeats.is_proxy_protocol_hello.clone();
    let connect = ups_heartbeats.connect.clone();
    return Some((is_proxy_protocol_hello, connect));
}
