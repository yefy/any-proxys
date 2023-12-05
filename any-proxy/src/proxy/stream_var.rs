use crate::proxy::stream_info::StreamInfo;
use anyhow::anyhow;
use anyhow::Result;
use chrono::prelude::*;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

type VarFunc = fn(stream_info: &StreamInfo) -> Option<String>;

lazy_static! {
    pub static ref VAR_MAP: HashMap<&'static str, VarFunc> = {
        let mut map: HashMap<&'static str, VarFunc> = HashMap::new();
        map.insert("local_time", local_time);
        map.insert("ssl_domain", ssl_domain);
        map.insert("local_domain", local_domain);
        map.insert("remote_domain", remote_domain);
        map.insert("local_protocol", local_protocol);
        map.insert("upstream_protocol", upstream_protocol);
        map.insert("local_addr", local_addr);
        map.insert("local_ip", local_ip);
        map.insert("local_port", local_port);
        map.insert("remote_addr", remote_addr);
        map.insert("remote_ip", remote_ip);
        map.insert("remote_port", remote_port);
        map.insert("session_time", session_time);
        map.insert("request_id", request_id);
        map.insert("versions", versions);
        map.insert("client_addr", client_addr);
        map.insert("client_ip", client_ip);
        map.insert("client_port", client_port);
        map.insert("domain", domain);
        map.insert("status", status);
        map.insert("status_str", status_str);
        map.insert("upstream_host", upstream_host);
        map.insert("upstream_addr", upstream_addr);
        map.insert("upstream_ip", upstream_ip);
        map.insert("upstream_port", upstream_port);
        map.insert("upstream_connect_time", upstream_connect_time);
        map.insert("stream_work_times", stream_work_times);
        map.insert("client_bytes_sent", client_bytes_sent);
        map.insert("client_bytes_received", client_bytes_received);
        map.insert("upstream_bytes_sent", upstream_bytes_sent);
        map.insert("upstream_bytes_received", upstream_bytes_received);
        map.insert("is_open_ebpf", is_open_ebpf);
        map.insert("upstream_dispatch", upstream_dispatch);
        map.insert("is_proxy_protocol_hello", is_proxy_protocol_hello);
        map.insert("buffer_cache", buffer_cache);
        map.insert("upstream_curr_stream_size", upstream_curr_stream_size);
        map.insert("upstream_max_stream_size", upstream_max_stream_size);
        map.insert("write_max_block_time_ms", write_max_block_time_ms);
        map.insert("is_timeout_exit", is_timeout_exit);

        map.insert(
            "upstream_min_stream_cache_size",
            upstream_min_stream_cache_size,
        );

        map.insert("total_read_size", total_read_size);

        map.insert("total_write_size", total_write_size);
        map
    };
}

pub fn find(var_name: &str, stream_info: &StreamInfo) -> Result<Option<String>> {
    let value = VAR_MAP.get(var_name.trim());
    match value {
        None => return Err(anyhow!("err=>var not found => var_name:{}", var_name)),
        Some(value) => Ok(value(stream_info)),
    }
}

pub fn local_time(_stream_connect: &StreamInfo) -> Option<String> {
    let date_str = Local::now().to_string();
    Some(date_str[0..26].to_string())
}

pub fn ssl_domain(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.ssl_domain.is_none() {
        return None;
    }
    Some(stream_info.ssl_domain.clone().unwrap())
}

pub fn local_domain(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.local_domain.is_none() {
        return None;
    }
    Some(stream_info.local_domain.clone().unwrap())
}

pub fn remote_domain(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.remote_domain.is_none() {
        return None;
    }
    Some(stream_info.remote_domain.clone().unwrap())
}

pub fn local_protocol(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.protocol77.is_none() {
        Some(stream_info.server_stream_info.protocol7.to_string())
    } else {
        Some(format!(
            "{}_{}",
            stream_info.protocol77.as_ref().unwrap().to_string(),
            stream_info.server_stream_info.protocol7.to_string()
        ))
    }
}

pub fn upstream_protocol(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    if stream_info.protocol77.is_none() {
        Some(
            stream_info
                .upstream_connect_info
                .get()
                .protocol7
                .to_string(),
        )
    } else {
        Some(format!(
            "{}_{}",
            stream_info.protocol77.as_ref().unwrap().to_string(),
            stream_info
                .upstream_connect_info
                .get()
                .protocol7
                .to_string()
        ))
    }
}

pub fn local_addr(stream_info: &StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .to_string(),
    )
}

pub fn local_ip(stream_info: &StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .ip()
            .to_string(),
    )
}

pub fn local_port(stream_info: &StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .port()
            .to_string(),
    )
}

pub fn remote_addr(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.server_stream_info.remote_addr.to_string())
}

pub fn remote_ip(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.server_stream_info.remote_addr.ip().to_string())
}

pub fn remote_port(stream_info: &StreamInfo) -> Option<String> {
    Some(
        stream_info
            .server_stream_info
            .remote_addr
            .port()
            .to_string(),
    )
}

pub fn session_time(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.session_time.to_string())
}

pub fn request_id(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.request_id.clone())
}

pub fn versions(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.version.clone())
}

pub fn client_addr(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.client_addr.to_string())
}

pub fn client_ip(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.client_addr.ip().to_string())
}

pub fn client_port(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.client_addr.port().to_string())
}

pub fn domain(stream_info: &StreamInfo) -> Option<String> {
    let protocol_hello = stream_info.protocol_hello.get();
    if protocol_hello.is_none() {
        return None;
    }
    Some(protocol_hello.domain.clone())
}

pub fn status(stream_info: &StreamInfo) -> Option<String> {
    let err_status = stream_info.err_status.clone() as usize;
    Some(err_status.to_string())
}

pub fn status_str(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.err_status_str.is_none() {
        return None;
    }
    Some(stream_info.err_status_str.clone().unwrap())
}

pub fn upstream_host(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(stream_info.upstream_connect_info.get().domain.clone())
}

pub fn upstream_addr(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .get()
            .remote_addr
            .to_string(),
    )
}

pub fn upstream_ip(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .get()
            .remote_addr
            .ip()
            .to_string(),
    )
}

pub fn upstream_port(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(
        stream_info
            .upstream_connect_info
            .get()
            .remote_addr
            .port()
            .to_string(),
    )
}

pub fn upstream_connect_time(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(stream_info.upstream_connect_info.get().elapsed.to_string())
}

pub fn stream_work_times(stream_info: &StreamInfo) -> Option<String> {
    let mut datas = String::with_capacity(100);
    for (name, stream_work_time) in stream_info.stream_work_times.iter() {
        let v = format!("{}:{:.3},", name, stream_work_time);
        datas.push_str(&v);
    }
    Some(datas)
}

pub fn client_bytes_sent(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.client_stream_flow_info.get().write.to_string())
}

pub fn client_bytes_received(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.client_stream_flow_info.get().read.to_string())
}

pub fn upstream_bytes_sent(stream_info: &StreamInfo) -> Option<String> {
    Some(
        stream_info
            .upstream_stream_flow_info
            .get()
            .write
            .to_string(),
    )
}

pub fn upstream_bytes_received(stream_info: &StreamInfo) -> Option<String> {
    Some(stream_info.upstream_stream_flow_info.get().read.to_string())
}

pub fn is_open_ebpf(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.is_open_ebpf {
        return Some("ebpf".to_string());
    }
    return None;
}

pub fn upstream_dispatch(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.ups_dispatch.is_some() {
        return Some(format!("{:?}", stream_info.ups_dispatch.as_ref().unwrap()));
    }
    return None;
}

pub fn is_proxy_protocol_hello(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.is_proxy_protocol_hello {
        return Some("proxy_protocol_hello".to_string());
    }
    return None;
}

pub fn buffer_cache(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.buffer_cache.is_some() {
        return Some(stream_info.buffer_cache.clone().unwrap().to_string());
    }
    return None;
}

pub fn upstream_curr_stream_size(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .get()
        .peer_stream_size
        .is_none()
    {
        return None;
    }

    let peer_stream_size = stream_info
        .upstream_connect_info
        .get()
        .peer_stream_size
        .as_ref()
        .unwrap()
        .load(Ordering::SeqCst);

    return Some(peer_stream_size.to_string());
}

pub fn upstream_max_stream_size(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .get()
        .max_stream_size
        .is_none()
    {
        return None;
    }

    let upstream_connect_info = stream_info.upstream_connect_info.get();
    let max_stream_size = upstream_connect_info.max_stream_size.as_ref().unwrap();

    return Some(max_stream_size.to_string());
}

pub fn upstream_min_stream_cache_size(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }

    if stream_info
        .upstream_connect_info
        .get()
        .min_stream_cache_size
        .is_none()
    {
        return None;
    }
    let upstream_connect_info = stream_info.upstream_connect_info.get();
    let min_stream_cache_size = upstream_connect_info
        .min_stream_cache_size
        .as_ref()
        .unwrap();

    return Some(min_stream_cache_size.to_string());
}

pub fn write_max_block_time_ms(stream_info: &StreamInfo) -> Option<String> {
    if stream_info.write_max_block_time_ms == 0 {
        return None;
    }
    return Some(stream_info.write_max_block_time_ms.to_string());
}

pub fn is_timeout_exit(stream_info: &StreamInfo) -> Option<String> {
    if !stream_info.is_timeout_exit {
        return None;
    }
    return Some("timeout exit".to_string());
}

pub fn total_read_size(stream_info: &StreamInfo) -> Option<String> {
    return Some(stream_info.total_read_size.to_string());
}

pub fn total_write_size(stream_info: &StreamInfo) -> Option<String> {
    return Some(stream_info.total_write_size.to_string());
}
