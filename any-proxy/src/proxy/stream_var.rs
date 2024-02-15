use crate::proxy::stream_info::StreamInfo;
use crate::util::var::VarAnyData;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use chrono::prelude::*;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

type VarFunc = fn(stream_info: &StreamInfo) -> Option<VarAnyData>;

lazy_static! {
    pub static ref TIMEOUT_EXIT: ArcString = "timeout_exit".into();
    pub static ref PROXY_PROTOCOL_HELLO: ArcString = "proxy_protocol_hello".into();
    pub static ref EBPF: ArcString = "ebpf".into();
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
        map.insert("open_sendfile", open_sendfile);
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
        map.insert("upstream_protocol_hello_size", upstream_protocol_hello_size);
        map.insert("client_protocol_hello_size", client_protocol_hello_size);
        map.insert("stream_stream_info", stream_stream_info);
        map
    };
}

pub fn find(var_name: &str, stream_info: &StreamInfo) -> Result<Option<VarAnyData>> {
    let value = VAR_MAP.get(var_name.trim());
    match value {
        None => return Err(anyhow!("err=>var not found => var_name:{}", var_name)),
        Some(value) => Ok(value(stream_info)),
    }
}

pub fn local_time(_stream_connect: &StreamInfo) -> Option<VarAnyData> {
    let date_str = Local::now().to_string();
    Some(VarAnyData::Str(date_str[0..26].to_string()))
}

pub fn ssl_domain(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.ssl_domain.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.ssl_domain.clone().unwrap(),
    ))
}

pub fn local_domain(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.local_domain.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.local_domain.clone().unwrap(),
    ))
}

pub fn remote_domain(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.remote_domain.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.remote_domain.clone().unwrap(),
    ))
}

pub fn local_protocol(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.client_protocol77.is_none() {
        Some(VarAnyData::Str(
            stream_info.server_stream_info.protocol7.to_string(),
        ))
    } else {
        if stream_info.client_protocol7.is_some() {
            Some(VarAnyData::Str(format!(
                "{}_{}_{}",
                stream_info.client_protocol77.as_ref().unwrap().to_string(),
                stream_info.client_protocol7.as_ref().unwrap().to_string(),
                stream_info.server_stream_info.protocol7.to_string()
            )))
        } else {
            Some(VarAnyData::Str(format!(
                "{}_{}",
                stream_info.client_protocol77.as_ref().unwrap().to_string(),
                stream_info.server_stream_info.protocol7.to_string()
            )))
        }
    }
}

pub fn upstream_protocol(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    if stream_info.upstream_protocol77.is_none() {
        Some(VarAnyData::Str(
            stream_info
                .upstream_connect_info
                .get()
                .protocol7
                .to_string(),
        ))
    } else {
        Some(VarAnyData::Str(format!(
            "{}_{}",
            stream_info
                .upstream_protocol77
                .as_ref()
                .unwrap()
                .to_string(),
            stream_info
                .upstream_connect_info
                .get()
                .protocol7
                .to_string()
        )))
    }
}

pub fn local_addr(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::SocketAddr(
        stream_info.server_stream_info.local_addr.unwrap(),
    ))
}

pub fn local_ip(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::Str(
        stream_info
            .server_stream_info
            .local_addr
            .unwrap()
            .ip()
            .to_string(),
    ))
}

pub fn local_port(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::U16(
        stream_info.server_stream_info.local_addr.unwrap().port(),
    ))
}

pub fn remote_addr(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::SocketAddr(
        stream_info.server_stream_info.remote_addr.clone(),
    ))
}

pub fn remote_ip(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::Str(
        stream_info.server_stream_info.remote_addr.ip().to_string(),
    ))
}

pub fn remote_port(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::U16(
        stream_info.server_stream_info.remote_addr.port(),
    ))
}

pub fn session_time(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::F32(stream_info.session_time))
}

pub fn request_id(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::ArcString(protocol_hello.request_id.clone()))
}

pub fn versions(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::ArcString(protocol_hello.version.clone()))
}

pub fn client_addr(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::SocketAddr(protocol_hello.client_addr))
}

pub fn client_ip(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::Str(protocol_hello.client_addr.ip().to_string()))
}

pub fn client_port(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::U16(protocol_hello.client_addr.port()))
}

pub fn domain(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    let protocol_hello = stream_info.protocol_hello.get();
    Some(VarAnyData::ArcString(protocol_hello.domain.clone()))
}

pub fn status(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let err_status = stream_info.err_status.clone() as usize;
    Some(VarAnyData::Usize(err_status))
}

pub fn status_str(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.err_status_str.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.err_status_str.clone().unwrap(),
    ))
}

pub fn upstream_host(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.upstream_connect_info.get().domain.clone(),
    ))
}

pub fn upstream_addr(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(VarAnyData::SocketAddr(
        stream_info.upstream_connect_info.get().remote_addr,
    ))
}

pub fn upstream_ip(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(VarAnyData::Str(
        stream_info
            .upstream_connect_info
            .get()
            .remote_addr
            .ip()
            .to_string(),
    ))
}

pub fn upstream_port(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(VarAnyData::U16(
        stream_info.upstream_connect_info.get().remote_addr.port(),
    ))
}

pub fn upstream_connect_time(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.upstream_connect_info.is_none() {
        return None;
    }
    Some(VarAnyData::F32(
        stream_info.upstream_connect_info.get().elapsed,
    ))
}

pub fn stream_work_times(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let mut datas = String::with_capacity(100);
    for (name, stream_work_time) in stream_info.stream_work_times.iter() {
        let v = format!("{}:{:.3},", name, stream_work_time);
        datas.push_str(&v);
    }
    let mut c_datas = String::with_capacity(100);
    let mut s_datas = String::with_capacity(100);
    for (is_client, name, stream_work_time) in stream_info.stream_work_times2.iter() {
        let flag = if *is_client { "c" } else { "s" };
        let v = format!("{}_{}:{:.3},", flag, name, stream_work_time);
        if *is_client {
            c_datas.push_str(&v);
        } else {
            s_datas.push_str(&v);
        }
    }
    datas.push_str(&c_datas);
    datas.push_str(&s_datas);
    Some(VarAnyData::Str(datas))
}

pub fn client_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.client_stream_flow_info.get().write,
    ))
}

pub fn client_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.client_stream_flow_info.get().read,
    ))
}

pub fn upstream_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.upstream_stream_flow_info.get().write,
    ))
}

pub fn upstream_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.upstream_stream_flow_info.get().read,
    ))
}

pub fn is_open_ebpf(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.is_open_ebpf {
        return Some(VarAnyData::ArcString(EBPF.clone()));
    }
    return None;
}

pub fn open_sendfile(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.open_sendfile.is_some() {
        return Some(VarAnyData::Str(stream_info.open_sendfile.clone().unwrap()));
    }
    return None;
}

pub fn upstream_dispatch(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.ups_dispatch.is_some() {
        return Some(VarAnyData::Str(stream_info.ups_dispatch.clone().unwrap()));
    }
    return None;
}

pub fn is_proxy_protocol_hello(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.is_proxy_protocol_hello {
        return Some(VarAnyData::ArcString(PROXY_PROTOCOL_HELLO.clone()));
    }
    return None;
}

pub fn buffer_cache(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.buffer_cache.is_some() {
        return Some(VarAnyData::ArcStr(
            stream_info.buffer_cache.clone().unwrap(),
        ));
    }
    return None;
}

pub fn upstream_curr_stream_size(stream_info: &StreamInfo) -> Option<VarAnyData> {
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

    return Some(VarAnyData::Usize(peer_stream_size));
}

pub fn upstream_max_stream_size(stream_info: &StreamInfo) -> Option<VarAnyData> {
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
    let max_stream_size = upstream_connect_info.max_stream_size.clone().unwrap();

    return Some(VarAnyData::Usize(max_stream_size));
}

pub fn upstream_min_stream_cache_size(stream_info: &StreamInfo) -> Option<VarAnyData> {
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
    let min_stream_cache_size = upstream_connect_info.min_stream_cache_size.clone().unwrap();

    return Some(VarAnyData::Usize(min_stream_cache_size));
}

pub fn write_max_block_time_ms(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.write_max_block_time_ms == 0 {
        return None;
    }
    return Some(VarAnyData::U128(stream_info.write_max_block_time_ms));
}

pub fn is_timeout_exit(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if !stream_info.is_timeout_exit {
        return None;
    }
    return Some(VarAnyData::ArcString(TIMEOUT_EXIT.clone()));
}

pub fn upstream_protocol_hello_size(stream_info: &StreamInfo) -> Option<VarAnyData> {
    return Some(VarAnyData::Usize(stream_info.upstream_protocol_hello_size));
}

pub fn client_protocol_hello_size(stream_info: &StreamInfo) -> Option<VarAnyData> {
    return Some(VarAnyData::Usize(stream_info.client_protocol_hello_size));
}

pub fn stream_stream_info(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let ssc_upload = if stream_info.ssc_upload.is_some() {
        let ssc_upload = stream_info.ssc_upload.get();
        format!(
            "ssc_upload:{:?}, max_stream_cache_size:{}, max_tmp_file_size:{}",
            &*ssc_upload.ssd.get(),
            ssc_upload.cs.max_stream_cache_size,
            ssc_upload.cs.max_tmp_file_size
        )
    } else {
        String::new()
    };

    let ssc_download = if stream_info.ssc_download.is_some() {
        let ssc_download = stream_info.ssc_download.get();
        format!(
            "ssc_download:{:?}, max_stream_cache_size:{}, max_tmp_file_size:{}",
            &*ssc_download.ssd.get(),
            ssc_download.cs.max_stream_cache_size,
            ssc_download.cs.max_tmp_file_size
        )
    } else {
        String::new()
    };

    let data = format!("{:?}, {:?}", ssc_upload, ssc_download);
    return Some(VarAnyData::Str(data));
}
