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
type VarHttpFunc = fn(name: &str, stream_info: &StreamInfo) -> Option<VarAnyData>;

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

        map.insert(
            "http_header_client_bytes_sent",
            http_header_client_bytes_sent,
        );
        map.insert(
            "http_header_client_bytes_received",
            http_header_client_bytes_received,
        );
        map.insert(
            "http_header_upstream_bytes_sent",
            http_header_upstream_bytes_sent,
        );
        map.insert(
            "http_header_upstream_bytes_received",
            http_header_upstream_bytes_received,
        );

        map.insert("http_body_client_bytes_sent", http_body_client_bytes_sent);
        map.insert(
            "http_body_client_bytes_received",
            http_body_client_bytes_received,
        );
        map.insert(
            "http_body_upstream_bytes_sent",
            http_body_upstream_bytes_sent,
        );
        map.insert(
            "http_body_upstream_bytes_received",
            http_body_upstream_bytes_received,
        );

        map.insert("is_open_ebpf", is_open_ebpf);
        map.insert("open_sendfile", open_sendfile);
        map.insert("upstream_balancer", upstream_balancer);
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
        map.insert("http_local_cache_req_count", http_local_cache_req_count);
        map.insert("http_cache_status", http_cache_status);
        map.insert("http_cache_file_status", http_cache_file_status);
        map.insert("http_is_upstream", http_is_upstream);
        map.insert("http_is_cache", http_is_cache);
        map.insert(
            "http_last_slice_upstream_index",
            http_last_slice_upstream_index,
        );
        map.insert("http_max_upstream_count", http_max_upstream_count);
        map
    };
    pub static ref HTTP_REQUEST_FLAG: &'static str = "http_request_";
    pub static ref HTTP_UPS_REQUEST_FLAG: &'static str = "http_ups_request_";
    pub static ref HTTP_RESPONSE_FLAG: &'static str = "http_response_";
    pub static ref HTTP_VAR_MAP: HashMap<&'static str, VarHttpFunc> = {
        let mut map: HashMap<&'static str, VarHttpFunc> = HashMap::new();
        map.insert("http_request", http_request);
        map.insert("http_ups_request", http_ups_request);
        map.insert("http_response", http_response);
        map
    };
}

pub fn find(var_name: &str, stream_info: &StreamInfo) -> Result<Option<VarAnyData>> {
    if var_name.len() > HTTP_REQUEST_FLAG.len() {
        if &var_name[0..HTTP_REQUEST_FLAG.len()] == *HTTP_REQUEST_FLAG {
            let var_name = &var_name[HTTP_REQUEST_FLAG.len()..];
            return Ok(http_request(var_name, stream_info));
        }
    }

    if var_name.len() > HTTP_UPS_REQUEST_FLAG.len() {
        if &var_name[0..HTTP_UPS_REQUEST_FLAG.len()] == *HTTP_UPS_REQUEST_FLAG {
            let var_name = &var_name[HTTP_UPS_REQUEST_FLAG.len()..];
            return Ok(http_ups_request(var_name, stream_info));
        }
    }

    if var_name.len() > HTTP_RESPONSE_FLAG.len() {
        if &var_name[0..HTTP_RESPONSE_FLAG.len()] == *HTTP_RESPONSE_FLAG {
            let var_name = &var_name[HTTP_RESPONSE_FLAG.len()..];
            return Ok(http_response(var_name, stream_info));
        }
    }

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
        if stream_info.client_protocol7.is_some() {
            Some(VarAnyData::Str(format!(
                "{}_{}",
                stream_info.server_stream_info.protocol7.to_string(),
                stream_info.client_protocol7.as_ref().unwrap().to_string(),
            )))
        } else {
            Some(VarAnyData::Str(
                stream_info.server_stream_info.protocol7.to_string(),
            ))
        }
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
    Some(VarAnyData::ArcString(
        stream_info.protocol_hello.request_id.clone(),
    ))
}

pub fn versions(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.protocol_hello.version.clone(),
    ))
}

pub fn client_addr(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    Some(VarAnyData::SocketAddr(
        stream_info.protocol_hello.client_addr,
    ))
}

pub fn client_ip(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    Some(VarAnyData::Str(
        stream_info.protocol_hello.client_addr.ip().to_string(),
    ))
}

pub fn client_port(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    Some(VarAnyData::U16(
        stream_info.protocol_hello.client_addr.port(),
    ))
}

pub fn domain(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.protocol_hello.is_none() {
        return None;
    }
    Some(VarAnyData::ArcString(
        stream_info.protocol_hello.domain.clone(),
    ))
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
    let mut n = stream_info.client_stream_flow_info.get().write;
    if stream_info.http_r.is_some() {
        n += stream_info.http_r.ctx.get().r_out.head_size as i64;
    }

    Some(VarAnyData::I64(n))
}

pub fn client_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let mut n = stream_info.client_stream_flow_info.get().read;
    if stream_info.http_r.is_some() {
        n += stream_info.http_r.ctx.get().r_in.head_size as i64;
    }

    Some(VarAnyData::I64(n))
}

pub fn upstream_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let mut n = stream_info.upstream_stream_flow_info.get().write;
    if stream_info.http_r.is_some() {
        n += stream_info.http_r.ctx.get().r_in.head_upstream_size as i64;
    }

    Some(VarAnyData::I64(n))
}

pub fn upstream_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    let mut n = stream_info.upstream_stream_flow_info.get().read;
    if stream_info.http_r.is_some() {
        n += stream_info.http_r.ctx.get().r_out.head_upstream_size as i64;
    }

    Some(VarAnyData::I64(n))
}

pub fn http_header_client_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    Some(VarAnyData::Usize(
        stream_info.http_r.ctx.get().r_out.head_size,
    ))
}

pub fn http_header_client_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    Some(VarAnyData::Usize(
        stream_info.http_r.ctx.get().r_in.head_size,
    ))
}

pub fn http_header_upstream_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    Some(VarAnyData::Usize(
        stream_info.http_r.ctx.get().r_in.head_upstream_size,
    ))
}

pub fn http_header_upstream_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    Some(VarAnyData::Usize(
        stream_info.http_r.ctx.get().r_out.head_upstream_size,
    ))
}

pub fn http_body_client_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.client_stream_flow_info.get().write,
    ))
}

pub fn http_body_client_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.client_stream_flow_info.get().read,
    ))
}

pub fn http_body_upstream_bytes_sent(stream_info: &StreamInfo) -> Option<VarAnyData> {
    Some(VarAnyData::I64(
        stream_info.upstream_stream_flow_info.get().write,
    ))
}

pub fn http_body_upstream_bytes_received(stream_info: &StreamInfo) -> Option<VarAnyData> {
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

pub fn upstream_balancer(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.ups_balancer.is_some() {
        return Some(VarAnyData::ArcString(
            stream_info.ups_balancer.clone().unwrap(),
        ));
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
        let ssc_upload = &stream_info.ssc_upload;
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
        let ssc_download = &stream_info.ssc_download;
        format!(
            "ssc_download:{:?}, max_stream_cache_size:{}, max_tmp_file_size:{}",
            &*ssc_download.ssd.get(),
            ssc_download.cs.max_stream_cache_size,
            ssc_download.cs.max_tmp_file_size
        )
    } else {
        String::new()
    };

    let data = format!("{:?}___{:?}", ssc_upload, ssc_download);
    return Some(VarAnyData::Str(data));
}

pub fn http_local_cache_req_count(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    return Some(VarAnyData::Usize(
        stream_info.http_r.local_cache_req_count.clone(),
    ));
}

pub fn http_cache_status(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    return Some(VarAnyData::Str(format!(
        "{:?}_{:?}",
        http_r_ctx.r_in.main.http_cache_status, http_r_ctx.r_in.http_cache_status
    )));
}

pub fn http_cache_file_status(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }

    if stream_info.http_r.http_cache_file.is_none() {
        return None;
    }
    let http_cache_file_ctx = &*stream_info.http_r.http_cache_file.ctx_thread.get();
    if http_cache_file_ctx.cache_file_status.is_none() {
        return None;
    }
    return Some(VarAnyData::Str(format!(
        "{:?}",
        http_cache_file_ctx.cache_file_status
    )));
}

pub fn http_is_upstream(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    return Some(VarAnyData::Bool(http_r_ctx.is_upstream));
}

pub fn http_is_cache(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    if http_r_ctx.r_out_main.is_none() {
        return None;
    }
    let r_out_main = http_r_ctx.r_out_main.as_ref().unwrap();
    return Some(VarAnyData::Bool(r_out_main.is_cache));
}

pub fn http_last_slice_upstream_index(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    return Some(VarAnyData::I32(http_r_ctx.last_slice_upstream_index));
}

pub fn http_max_upstream_count(stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    return Some(VarAnyData::I64(http_r_ctx.max_upstream_count));
}

pub fn http_request(name: &str, stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();

    if name == "method" {
        return Some(VarAnyData::Str(http_r_ctx.r_in.method.to_string()));
    } else if name == "url" {
        return Some(VarAnyData::Str(http_r_ctx.r_in.uri.to_string()));
    } else if name == "uri" {
        let uri = http_r_ctx
            .r_in
            .uri
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
            .to_string();
        return Some(VarAnyData::Str(uri));
    } else if name == "version" {
        return Some(VarAnyData::Str(format!("{:?}", http_r_ctx.r_in.version)));
    } else if name == "host" {
        let domain = http_r_ctx.r_in.uri.host().unwrap();
        let port_u16 = http_r_ctx.r_in.uri.port_u16().unwrap();
        return Some(VarAnyData::Str(format!("{}:{}", domain, port_u16)));
    } else if name == "scheme" {
        let scheme = http_r_ctx.r_in.uri.scheme_str().unwrap().to_string();
        return Some(VarAnyData::Str(scheme));
    } else if name == "domain" {
        let domain = http_r_ctx.r_in.uri.host().unwrap();
        return Some(VarAnyData::Str(domain.to_string()));
    } else if name == "port" {
        let port_u16 = http_r_ctx.r_in.uri.port_u16().unwrap();
        return Some(VarAnyData::U16(port_u16));
    }

    let value = http_r_ctx.r_in.headers.get(name).cloned();
    if value.is_none() {
        return None;
    }
    let value = value.unwrap();
    return Some(VarAnyData::HeaderValue(value));
}

pub fn http_ups_request(name: &str, stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();

    if name == "method" {
        return Some(VarAnyData::Str(http_r_ctx.r_in.method_upstream.to_string()));
    } else if name == "url" {
        return Some(VarAnyData::Str(http_r_ctx.r_in.uri_upstream.to_string()));
    } else if name == "uri" {
        let uri = http_r_ctx
            .r_in
            .uri_upstream
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
            .to_string();
        return Some(VarAnyData::Str(uri));
    } else if name == "version" {
        return Some(VarAnyData::Str(format!(
            "{:?}",
            http_r_ctx.r_in.version_upstream
        )));
    } else if name == "host" {
        let domain = http_r_ctx.r_in.uri_upstream.host().unwrap();
        let port_u16 = http_r_ctx.r_in.uri_upstream.port_u16().unwrap();
        return Some(VarAnyData::Str(format!("{}:{}", domain, port_u16)));
    } else if name == "scheme" {
        let scheme = http_r_ctx
            .r_in
            .uri_upstream
            .scheme_str()
            .unwrap()
            .to_string();
        return Some(VarAnyData::Str(scheme));
    } else if name == "domain" {
        let domain = http_r_ctx.r_in.uri_upstream.host().unwrap();
        return Some(VarAnyData::Str(domain.to_string()));
    } else if name == "port" {
        let port_u16 = http_r_ctx.r_in.uri_upstream.port_u16().unwrap();
        return Some(VarAnyData::U16(port_u16));
    }

    let value = http_r_ctx.r_in.headers_upstream.get(name).cloned();
    if value.is_none() {
        return None;
    }
    let value = value.unwrap();
    return Some(VarAnyData::HeaderValue(value));
}

pub fn http_response(name: &str, stream_info: &StreamInfo) -> Option<VarAnyData> {
    if stream_info.http_r.is_none() {
        return None;
    }
    let http_r_ctx = &*stream_info.http_r.ctx.get();
    if name == "status" {
        return Some(VarAnyData::Str(http_r_ctx.r_out.status.to_string()));
    } else if name == "version" {
        return Some(VarAnyData::Str(format!("{:?}", http_r_ctx.r_out.version)));
    }

    let value = http_r_ctx.r_out.headers.get(name).cloned();
    if value.is_none() {
        return None;
    }
    let value = value.unwrap();
    return Some(VarAnyData::HeaderValue(value));
}
