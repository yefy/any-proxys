use crate::proxy::stream_connect;
use crate::util::var;
use anyhow::Result;
use chrono::prelude::*;
use std::collections::HashMap;

type VarFunc = fn(stream_connect: &stream_connect::StreamConnect) -> Option<String>;
type VarHashMap = HashMap<&'static str, VarFunc>;

pub struct StreamVar {
    pub var_map: VarHashMap,
}

impl StreamVar {
    pub fn new() -> StreamVar {
        let mut var_map = HashMap::<&'static str, VarFunc>::new();

        var_map.insert("local_time", local_time);
        var_map.insert("ssl_domain", ssl_domain);
        var_map.insert("local_domain", local_domain);
        var_map.insert("remote_domain", remote_domain);
        var_map.insert("local_protocol", local_protocol);
        var_map.insert("upstream_protocol", upstream_protocol);
        var_map.insert("local_addr", local_addr);
        var_map.insert("local_ip", local_ip);
        var_map.insert("local_port", local_port);
        var_map.insert("remote_addr", remote_addr);
        var_map.insert("remote_ip", remote_ip);
        var_map.insert("remote_port", remote_port);
        var_map.insert("session_time", session_time);
        var_map.insert("request_id", request_id);
        var_map.insert("versions", versions);
        var_map.insert("client_addr", client_addr);
        var_map.insert("client_ip", client_ip);
        var_map.insert("client_port", client_port);
        var_map.insert("domain", domain);
        var_map.insert("status", status);
        var_map.insert("status_str", status_str);
        var_map.insert("upstream_addr", upstream_addr);
        var_map.insert("upstream_ip", upstream_ip);
        var_map.insert("upstream_port", upstream_port);
        var_map.insert("upstream_connect_time", upstream_connect_time);
        var_map.insert("stream_work_times", stream_work_times);
        var_map.insert("client_bytes_sent", client_bytes_sent);
        var_map.insert("client_bytes_received", client_bytes_received);
        var_map.insert("upstream_bytes_sent", upstream_bytes_sent);
        var_map.insert("upstream_bytes_received", upstream_bytes_received);

        StreamVar { var_map }
    }

    pub fn find(
        &self,
        var_name: &str,
        stream_connect: &stream_connect::StreamConnect,
    ) -> Result<Option<String>> {
        let value = self.var_map.get(var_name.trim());
        match value {
            None => {
                return Err(anyhow::anyhow!(
                    "err=>var not found => var_name:{}",
                    var_name
                ))
            }
            Some(value) => Ok(value(stream_connect)),
        }
    }
}

pub fn local_time(_stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    let date_str = Local::now().to_string();
    Some(date_str[0..26].to_string())
}

pub fn ssl_domain(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.ssl_domain.is_none() {
        return None;
    }
    Some(stream_connect.ssl_domain.clone().unwrap())
}

pub fn local_domain(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.local_domain.is_none() {
        return None;
    }
    Some(stream_connect.local_domain.clone().unwrap())
}

pub fn remote_domain(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.remote_domain.is_none() {
        return None;
    }
    Some(stream_connect.remote_domain.clone().unwrap())
}

pub fn local_protocol(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.local_protocol_name.clone())
}

pub fn upstream_protocol(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.upstream_protocol_name.is_none() {
        return None;
    }
    Some(stream_connect.upstream_protocol_name.clone().unwrap())
}

pub fn local_addr(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.local_addr.to_string())
}

pub fn local_ip(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.local_addr.ip().to_string())
}

pub fn local_port(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.local_addr.port().to_string())
}

pub fn remote_addr(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.remote_addr.to_string())
}

pub fn remote_ip(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.remote_addr.ip().to_string())
}

pub fn remote_port(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.remote_addr.port().to_string())
}

pub fn session_time(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(stream_connect.session_time.to_string())
}

pub fn request_id(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .request_id
            .clone(),
    )
}

pub fn versions(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .version
            .clone(),
    )
}

pub fn client_addr(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .client_addr
            .to_string(),
    )
}

pub fn client_ip(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .client_addr
            .ip()
            .to_string(),
    )
}

pub fn client_port(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .client_addr
            .port()
            .to_string(),
    )
}

pub fn domain(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.protocol_hello.is_none() {
        return None;
    }
    Some(
        stream_connect
            .protocol_hello
            .as_ref()
            .unwrap()
            .domain
            .clone(),
    )
}

pub fn status(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    let err_status = stream_connect.err_status.clone() as usize;
    Some(err_status.to_string())
}

pub fn status_str(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.err_status_str.is_none() {
        return None;
    }
    Some(stream_connect.err_status_str.clone().unwrap())
}

pub fn upstream_addr(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.upstream_addr.is_none() {
        return None;
    }
    Some(stream_connect.upstream_addr.clone().unwrap())
}

pub fn upstream_ip(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.upstream_addr.is_none() {
        return None;
    }
    let (ip, _) = var::Var::host_and_port(stream_connect.upstream_addr.as_ref().unwrap());
    Some(ip.to_string())
}

pub fn upstream_port(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.upstream_addr.is_none() {
        return None;
    }
    let (_, port) = var::Var::host_and_port(stream_connect.upstream_addr.as_ref().unwrap());
    Some(port.to_string())
}

pub fn upstream_connect_time(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    if stream_connect.upstream_connect_time.is_none() {
        return None;
    }
    Some(
        stream_connect
            .upstream_connect_time
            .clone()
            .unwrap()
            .to_string(),
    )
}

pub fn stream_work_times(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    let mut datas = String::with_capacity(100);
    for (name, stream_work_time) in stream_connect.stream_work_times.borrow().iter() {
        let v = format!("{}:{:.3},", name, stream_work_time);
        datas.push_str(&v);
    }
    Some(datas)
}

pub fn client_bytes_sent(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(
        stream_connect
            .client_stream_info
            .lock()
            .unwrap()
            .write
            .to_string(),
    )
}

pub fn client_bytes_received(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(
        stream_connect
            .client_stream_info
            .lock()
            .unwrap()
            .read
            .to_string(),
    )
}

pub fn upstream_bytes_sent(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(
        stream_connect
            .upstream_stream_info
            .lock()
            .unwrap()
            .write
            .to_string(),
    )
}

pub fn upstream_bytes_received(stream_connect: &stream_connect::StreamConnect) -> Option<String> {
    Some(
        stream_connect
            .upstream_stream_info
            .lock()
            .unwrap()
            .read
            .to_string(),
    )
}
