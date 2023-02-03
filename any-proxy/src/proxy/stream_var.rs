use crate::proxy::stream_info;
use anyhow::anyhow;
use anyhow::Result;
use chrono::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

type VarFunc = fn(stream_info: &stream_info::StreamInfo) -> Option<String>;
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
        var_map.insert("upstream_host", upstream_host);
        var_map.insert("upstream_addr", upstream_addr);
        var_map.insert("upstream_ip", upstream_ip);
        var_map.insert("upstream_port", upstream_port);
        var_map.insert("upstream_connect_time", upstream_connect_time);
        var_map.insert("stream_work_times", stream_work_times);
        var_map.insert("client_bytes_sent", client_bytes_sent);
        var_map.insert("client_bytes_received", client_bytes_received);
        var_map.insert("upstream_bytes_sent", upstream_bytes_sent);
        var_map.insert("upstream_bytes_received", upstream_bytes_received);
        var_map.insert("is_open_ebpf", is_open_ebpf);
        var_map.insert("upstream_dispatch", upstream_dispatch);
        var_map.insert("is_proxy_protocol_hello", is_proxy_protocol_hello);
        var_map.insert("buffer_cache", buffer_cache);
        var_map.insert("upstream_curr_stream_size", upstream_curr_stream_size);
        var_map.insert("upstream_max_stream_size", upstream_max_stream_size);
        var_map.insert("write_max_block_time_ms", write_max_block_time_ms);

        var_map.insert(
            "upstream_min_stream_cache_size",
            upstream_min_stream_cache_size,
        );

        StreamVar { var_map }
    }

    pub fn find(
        &self,
        var_name: &str,
        stream_info: &stream_info::StreamInfo,
    ) -> Result<Option<String>> {
        let value = self.var_map.get(var_name.trim());
        match value {
            None => return Err(anyhow!("err=>var not found => var_name:{}", var_name)),
            Some(value) => Ok(value(stream_info)),
        }
    }
}

pub fn local_time(_stream_connect: &stream_info::StreamInfo) -> Option<String> {
    let date_str = Local::now().to_string();
    Some(date_str[0..26].to_string())
}

pub fn ssl_domain(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.ssl_domain.is_none() {
        return None;
    }
    Some(stream_info.ssl_domain.clone().unwrap())
}

pub fn local_domain(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.local_domain.is_none() {
        return None;
    }
    Some(stream_info.local_domain.clone().unwrap())
}

pub fn remote_domain(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.remote_domain.is_none() {
        return None;
    }
    Some(stream_info.remote_domain.clone().unwrap())
}

pub fn local_protocol(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(stream_info.server_stream_info.protocol7.to_string())
}

pub fn upstream_protocol(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .protocol7
            .to_string(),
    )
}

pub fn local_addr(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .to_string(),
    )
}

pub fn local_ip(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .ip()
            .to_string(),
    )
}

pub fn local_port(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .port()
            .to_string(),
    )
}

pub fn remote_addr(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(stream_info.server_stream_info.remote_addr.to_string())
}

pub fn remote_ip(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(stream_info.server_stream_info.remote_addr.ip().to_string())
}

pub fn remote_port(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .remote_addr
            .port()
            .to_string(),
    )
}

pub fn session_time(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(stream_info.session_time.to_string())
}

pub fn request_id(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.as_ref().unwrap().request_id.clone())
}

pub fn versions(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.as_ref().unwrap().version.clone())
}

pub fn client_addr(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.as_ref().unwrap().client_addr.to_string())
}

pub fn client_ip(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(
        protocol_hello
            .as_ref()
            .unwrap()
            .client_addr
            .ip()
            .to_string(),
    )
}

pub fn client_port(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(
        protocol_hello
            .as_ref()
            .unwrap()
            .client_addr
            .port()
            .to_string(),
    )
}

pub fn domain(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.lock().unwrap();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.as_ref().unwrap().domain.clone())
}

pub fn status(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let err_status = stream_info.err_status.clone() as usize;
    Some(err_status.to_string())
}

pub fn status_str(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.err_status_str.is_none() {
        return None;
    }
    Some(stream_info.err_status_str.clone().unwrap())
}

pub fn upstream_host(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .domain
            .clone(),
    )
}

pub fn upstream_addr(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .remote_addr
            .to_string(),
    )
}

pub fn upstream_ip(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .remote_addr
            .ip()
            .to_string(),
    )
}

pub fn upstream_port(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .remote_addr
            .port()
            .to_string(),
    )
}

pub fn upstream_connect_time(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .elapsed
            .to_string(),
    )
}

pub fn stream_work_times(stream_info: &stream_info::StreamInfo) -> Option<String> {
    let mut datas = String::with_capacity(100);
    for (name, stream_work_time) in stream_info.stream_work_times.iter() {
        let v = format!("{}:{:.3},", name, stream_work_time);
        datas.push_str(&v);
    }
    Some(datas)
}

pub fn client_bytes_sent(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .client_stream_flow_info
            .lock()
            .unwrap()
            .write
            .to_string(),
    )
}

pub fn client_bytes_received(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .client_stream_flow_info
            .lock()
            .unwrap()
            .read
            .to_string(),
    )
}

pub fn upstream_bytes_sent(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .upstream_stream_flow_info
            .lock()
            .unwrap()
            .write
            .to_string(),
    )
}

pub fn upstream_bytes_received(stream_info: &stream_info::StreamInfo) -> Option<String> {
    Some(
        stream_info
            .upstream_stream_flow_info
            .lock()
            .unwrap()
            .read
            .to_string(),
    )
}

pub fn is_open_ebpf(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.is_open_ebpf {
        return Some("ebpf".to_string());
    }
    return None;
}

pub fn upstream_dispatch(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.ups_dispatch.is_some() {
        return Some(format!("{:?}", stream_info.ups_dispatch.as_ref().unwrap()));
    }
    return None;
}

pub fn is_proxy_protocol_hello(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.is_proxy_protocol_hello {
        return Some("proxy_protocol_hello".to_string());
    }
    return None;
}

pub fn buffer_cache(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.buffer_cache.is_some() {
        return stream_info.buffer_cache.clone();
    }
    return None;
}

pub fn upstream_curr_stream_size(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .peer_stream_size
        .is_none()
    {
        return None;
    }

    let peer_stream_size = stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .peer_stream_size
        .as_ref()
        .unwrap()
        .load(Ordering::SeqCst);

    return Some(peer_stream_size.to_string());
}

pub fn upstream_max_stream_size(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .max_stream_size
        .is_none()
    {
        return None;
    }

    let max_stream_size = stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .max_stream_size
        .as_ref()
        .unwrap();

    return Some(max_stream_size.to_string());
}

pub fn upstream_min_stream_cache_size(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .min_stream_cache_size
        .is_none()
    {
        return None;
    }

    let min_stream_cache_size = stream_info
        .upstream_connect_info
        .as_ref()
        .unwrap()
        .min_stream_cache_size
        .as_ref()
        .unwrap();

    return Some(min_stream_cache_size.to_string());
}

pub fn write_max_block_time_ms(stream_info: &stream_info::StreamInfo) -> Option<String> {
    if stream_info.write_max_block_time_ms == 0 {
        return None;
    }
    return Some(stream_info.write_max_block_time_ms.to_string());
}
