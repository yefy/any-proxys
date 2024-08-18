use crate::config::net_core::DomainFromHttpV1;
use crate::protopack::anyproxy::{AnyproxyHello, ANYPROXY_VERSION};
use crate::proxy::http_proxy::http_stream_request::{
    HttpResponse, HttpResponseBody, HttpStreamRequest, HttpUpstreamConnectInfo,
};
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::stream_info::{ErrStatus, StreamInfo};
use crate::proxy::websocket_proxy::WebSocketStreamTrait;
use crate::proxy::{stream_var, ServerArg, StreamConfigContext};
use crate::stream::connect::Connect;
use crate::util::var::Var;
use crate::wasm::WasmStreamInfo;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ArcMutexTokio, ArcUnsafeAny, Share};
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use http::Response;
use hyper::Body;
use std::future::Future;
use std::sync::Arc;
use tokio_tungstenite::WebSocketStream;

pub async fn parse_proxy_domain<S, SF, D, DF>(
    arg: &ServerArg,
    hello_service: S,
    domain_service: D,
) -> Result<Arc<StreamConfigContext>>
where
    S: FnOnce() -> SF,
    SF: Future<Output = Result<Option<(AnyproxyHello, usize)>>>,
    D: FnOnce(Arc<DomainFromHttpV1>) -> DF,
    DF: Future<Output = Result<ArcString>>,
{
    arg.stream_info.get_mut().add_work_time1("hello");
    let hello = {
        arg.stream_info.get_mut().err_status = ErrStatus::CLIENT_PROTO_ERR;
        let hello = hello_service().await?;
        match hello {
            Some((mut hello, hello_pack_size)) => {
                arg.stream_info.get_mut().client_protocol_hello_size = hello_pack_size;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.get_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.get_mut().ssl_domain = Some(hello.domain.clone());
                }
                arg.stream_info.get_mut().remote_domain = Some(hello.domain.clone());
                arg.stream_info.get_mut().local_domain = Some(hello.domain.clone());
                let session_id = arg.stream_info.get().session_id;
                let (id, _) = hello
                    .request_id
                    .split_once("@")
                    .ok_or(anyhow!("request_id"))?;
                hello.request_id = ArcString::new(format!("{}@{}", id, session_id));
                hello
            }
            None => {
                let domain =
                    domain_service(arg.domain_config_listen.domain_from_http_v1.clone()).await?;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.get_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.get_mut().ssl_domain = Some(domain.clone());
                }
                arg.stream_info.get_mut().ssl_domain = Some(domain.clone());
                arg.stream_info.get_mut().remote_domain = Some(domain.clone());
                arg.stream_info.get_mut().local_domain = Some(domain.clone());
                let session_id = arg.stream_info.get().session_id;

                let server_stream_info = arg.stream_info.get().server_stream_info.clone();
                AnyproxyHello {
                    version: ANYPROXY_VERSION.into(),
                    request_id: format!(
                        "{}_{}_{}_{}@{}",
                        session_id,
                        server_stream_info.local_addr.clone().unwrap(),
                        server_stream_info.remote_addr,
                        unsafe { libc::getpid() },
                        session_id,
                    )
                    .into(),
                    client_addr: server_stream_info.remote_addr.clone(),
                    domain,
                }
            }
        }
    };

    let domain_index = {
        let mut stream_info = arg.stream_info.get_mut();
        stream_info.request_id = hello.request_id.clone();
        stream_info.protocol_hello.set(Arc::new(hello));

        arg.domain_config_listen
            .domain_index
            .index(&stream_info.protocol_hello.domain)
            .map_err(|e| anyhow!("err:domain_index.index => e:{}", e))?
    };
    let domain_config_context = arg
        .domain_config_listen
        .domain_config_context_map
        .get(&domain_index)
        .cloned()
        .ok_or(anyhow!(
            "err:domain_index nil => domain_index:{}",
            domain_index
        ))?;

    let scc = &domain_config_context.scc;
    let scc = {
        let scc = scc.to_arc_scc(arg.ms.clone());
        let mut stream_info = arg.stream_info.get_mut();
        let net_core_conf = scc.net_core_conf();
        stream_info.scc = Some(scc.clone()).into();
        stream_info.debug_is_open_print = net_core_conf.debug_is_open_print;
        stream_info.debug_is_open_stream_work_times = net_core_conf.debug_is_open_stream_work_times;
        stream_info.debug_print_access_log_time = net_core_conf.debug_print_access_log_time;
        stream_info.debug_print_stream_flow_time = net_core_conf.debug_print_stream_flow_time;
        stream_info.stream_so_singer_time = net_core_conf.stream_so_singer_time;
        scc
    };
    Ok(scc)
}

pub async fn upstream_connect_info(
    stream_info: &Share<StreamInfo>,
    scc: &Arc<StreamConfigContext>,
) -> Result<HttpUpstreamConnectInfo> {
    stream_info.get_mut().err_status = ErrStatus::SERVICE_UNAVAILABLE;
    use crate::config::net_server_core;
    let net_server_core_conf = net_server_core::curr_conf(scc.net_curr_conf());
    if net_server_core_conf.upstream_var_addr.is_some() {
        let upstream_var_addr = net_server_core_conf.upstream_var_addr.clone();
        let ms = scc.ms();
        let (is_proxy_protocol_hello, connect_func) =
            upstream_var_addr.get_connect(ms, &stream_info).await?;

        return Ok(HttpUpstreamConnectInfo::new(
            is_proxy_protocol_hello,
            false,
            Some(connect_func).into(),
            None,
        ));
    }

    if net_server_core_conf.upstream_data.is_none() {
        return Err(anyhow!("err:upstream_data.is_none"));
    }

    let client_ip = stream_var::client_ip(&stream_info.get());
    let client_ip = if client_ip.is_none() {
        stream_info.get().server_stream_info.remote_addr.to_string()
    } else {
        use crate::util::var::VarAnyData;
        let client_ip = client_ip.unwrap();
        if let VarAnyData::Str(client_ip) = client_ip {
            client_ip
        } else {
            return Err(anyhow!("err:client_ip"));
        }
    };

    let (ups_balancer, connect_info) = net_server_core_conf
        .upstream_data
        .get_mut()
        .balancer_and_connect(&client_ip)?;

    stream_info.get_mut().ups_balancer = Some(ups_balancer.clone());
    if connect_info.is_none() {
        return Err(anyhow!("err: connect_func nil"));
    }
    let (is_proxy_protocol_hello, is_connect_func_disable, connect_func) = connect_info.unwrap();
    Ok(HttpUpstreamConnectInfo::new(
        is_proxy_protocol_hello,
        is_connect_func_disable,
        Some(connect_func).into(),
        Some(ups_balancer),
    ))
}

pub async fn upstream_do_connect(
    stream_info: &Share<StreamInfo>,
    connect_func: &Arc<Box<dyn Connect>>,
) -> Result<StreamFlow> {
    let upstream_connect_flow_info = stream_info.get().upstream_connect_flow_info.clone();
    let request_id = stream_info.get().request_id.clone();
    let executors = stream_info.get().executors.clone().unwrap();
    let connect_info = connect_func
        .connect(
            Some(request_id.clone()),
            Some(upstream_connect_flow_info),
            Some(executors.context.run_time.clone()),
        )
        .await
        .map_err(|e| {
            anyhow!(
                "err:connect => request_id:{}, e:{}",
                stream_info.get().request_id,
                e
            )
        })?;

    let (mut upstream_stream, upstream_connect_info) = connect_info;

    {
        let stream_info = stream_info.get();
        log::trace!(target: "main",
            "skip ebpf warning request_id:{}, server_stream_info:{:?}, ups_local_addr:{}, ups_remote_addr:{}",
            stream_info.request_id,
            stream_info.server_stream_info,
            upstream_connect_info.local_addr,
            upstream_connect_info.remote_addr
        );
    }

    log::trace!(target: "main",
        "upstream_protocol_name:{}",
        upstream_connect_info.protocol7.to_string()
    );

    stream_info
        .get_mut()
        .upstream_connect_info
        .set(upstream_connect_info);
    upstream_stream.set_stream_info(Some(stream_info.get().upstream_stream_flow_info.clone()));

    Ok(upstream_stream)
}

pub async fn upstream_connect(
    stream_info: &Share<StreamInfo>,
    scc: &Arc<StreamConfigContext>,
) -> Result<(Option<bool>, StreamFlow)> {
    stream_info.get_mut().add_work_time1("upstream_connect");

    let upstream_connect_info = upstream_connect_info(&stream_info, scc).await?;
    if upstream_connect_info.is_connect_func_disable {
        return Err(anyhow!("err: connect_func nil"));
    }
    let connect_func = &upstream_connect_info.connect_func;

    let upstream_stream = upstream_do_connect(stream_info, connect_func).await?;

    Ok((
        upstream_connect_info.is_proxy_protocol_hello,
        upstream_stream,
    ))
}

pub async fn get_proxy_hello(
    is_proxy_protocol_hello: Option<bool>,
    stream_info: &Share<StreamInfo>,
    scc: &Arc<StreamConfigContext>,
) -> Option<Arc<AnyproxyHello>> {
    let is_proxy_protocol_hello = {
        let mut stream_info = stream_info.get_mut();
        stream_info.err_status = ErrStatus::OK;
        let is_proxy_protocol_hello = if is_proxy_protocol_hello.is_none() {
            scc.net_core_conf().is_proxy_protocol_hello
        } else {
            is_proxy_protocol_hello.unwrap()
        };
        stream_info.is_proxy_protocol_hello = is_proxy_protocol_hello;
        is_proxy_protocol_hello
    };

    if is_proxy_protocol_hello {
        let hello = { stream_info.get().protocol_hello.clone().unwrap() };
        Some(hello)
    } else {
        None
    }
}

pub async fn run_plugin_handle_access(
    scc: &Arc<StreamConfigContext>,
    stream_info: &Share<StreamInfo>,
) -> Result<bool> {
    let plugin_handle_access = {
        use crate::config::net_core_plugin;
        //___wait___
        //let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_core_conf());
        let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());
        net_core_plugin_conf.plugin_handle_access.clone()
    };

    for plugin_handle_access in &*plugin_handle_access.get().await {
        let err = (plugin_handle_access)(stream_info.clone()).await?;
        match err {
            crate::Error::Ok => {}
            crate::Error::Break => {}
            crate::Error::Finish => {
                break;
            }
            crate::Error::Error => {
                return Err(anyhow::anyhow!("err:plugin_handle_access"));
            }
            crate::Error::Return => {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub async fn run_plugin_handle_serverless(
    scc: &Arc<StreamConfigContext>,
    stream_info: &Share<StreamInfo>,
    client_buf_reader: any_base::io_rb::buf_reader::BufReader<StreamFlow>,
) -> Result<Option<any_base::io_rb::buf_reader::BufReader<StreamFlow>>> {
    use crate::config::net_core_plugin;
    let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());

    let ret: Result<bool> = async {
        for is_plugin_handle_serverless in
            &*net_core_plugin_conf.is_plugin_handle_serverless.get().await
        {
            let is_ok = (is_plugin_handle_serverless)(stream_info.clone()).await?;
            if is_ok {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    .await;
    let is_ok = ret?;
    if !is_ok {
        return Ok(Some(client_buf_reader));
    }

    stream_info.get_mut().err_status = ErrStatus::OK;
    //stream_info.get_mut().is_discard_flow = true;
    stream_info.get_mut().is_discard_timeout = true;
    let client_buf_reader =
        any_base::io::buf_reader::BufReader::from_rb_buf_reader(client_buf_reader);
    let client_buf_stream = any_base::io::buf_writer::BufWriter::new(client_buf_reader);
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(client_buf_stream);
    let client_buf_stream = ArcMutexTokio::new(client_buf_stream);
    let session_id = stream_info.get().session_id;

    let mut wasm_stream_info = WasmStreamInfo::new(None);
    wasm_stream_info
        .wasm_socket_map
        .insert(session_id, client_buf_stream);
    let wasm_stream_info = Share::new(wasm_stream_info);
    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .insert(session_id, wasm_stream_info.clone());
    stream_info.get_mut().wasm_stream_info = wasm_stream_info;

    let ret: Result<()> = async {
        for plugin_handle_serverless in &*net_core_plugin_conf.plugin_handle_serverless.get().await
        {
            let err = (plugin_handle_serverless)(stream_info.clone()).await?;
            match err {
                crate::Error::Ok => {}
                crate::Error::Break => {}
                crate::Error::Finish => {
                    break;
                }
                crate::Error::Error => {
                    return Err(anyhow::anyhow!("err:plugin_handle_serverless"));
                }
                crate::Error::Return => {
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    ret?;

    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .remove(&session_id);
    Ok(None)
}

pub async fn run_plugin_handle_websocket_serverless(
    scc: &Arc<StreamConfigContext>,
    stream_info: &Share<StreamInfo>,
    client_stream: WebSocketStream<BufStream<StreamFlow>>,
) -> Result<Option<WebSocketStream<BufStream<StreamFlow>>>> {
    use crate::config::net_core_plugin;
    let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());

    let ret: Result<bool> = async {
        for is_plugin_handle_serverless in
            &*net_core_plugin_conf.is_plugin_handle_serverless.get().await
        {
            let is_ok = (is_plugin_handle_serverless)(stream_info.clone()).await?;
            if is_ok {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    .await;
    let is_ok = ret?;
    if !is_ok {
        return Ok(Some(client_stream));
    }

    stream_info.get_mut().err_status = ErrStatus::OK;
    //stream_info.get_mut().is_discard_flow = true;
    stream_info.get_mut().is_discard_timeout = true;
    let client_stream: ArcMutexTokio<Box<dyn WebSocketStreamTrait>> =
        ArcMutexTokio::new(Box::new(client_stream));
    let session_id = stream_info.get().session_id;

    let mut wasm_stream_info = WasmStreamInfo::new(None);
    wasm_stream_info
        .wasm_websocket_map
        .insert(session_id, client_stream);
    let wasm_stream_info = Share::new(wasm_stream_info);
    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .insert(session_id, wasm_stream_info.clone());
    stream_info.get_mut().wasm_stream_info = wasm_stream_info;

    let ret: Result<()> = async {
        for plugin_handle_serverless in &*net_core_plugin_conf.plugin_handle_serverless.get().await
        {
            let err = (plugin_handle_serverless)(stream_info.clone()).await?;
            match err {
                crate::Error::Ok => {}
                crate::Error::Break => {}
                crate::Error::Finish => {
                    break;
                }
                crate::Error::Error => {
                    return Err(anyhow::anyhow!("err:plugin_handle_serverless"));
                }
                crate::Error::Return => {
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    ret?;

    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .remove(&session_id);
    Ok(None)
}

pub async fn run_plugin_handle_http_serverless(
    scc: &Arc<StreamConfigContext>,
    stream_info: &Share<StreamInfo>,
) -> Result<bool> {
    use crate::config::net_core_plugin;
    let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());

    let ret: Result<bool> = async {
        for is_plugin_handle_serverless in
            &*net_core_plugin_conf.is_plugin_handle_serverless.get().await
        {
            let is_ok = (is_plugin_handle_serverless)(stream_info.clone()).await?;
            if is_ok {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    .await;
    let is_ok = ret?;
    if !is_ok {
        return Ok(false);
    }

    stream_info.get_mut().err_status = ErrStatus::OK;
    //stream_info.get_mut().is_discard_flow = true;
    stream_info.get_mut().is_discard_timeout = true;
    let session_id = stream_info.get().session_id;

    let wasm_stream_info = Share::new(WasmStreamInfo::new(None));
    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .insert(session_id, wasm_stream_info.clone());
    stream_info.get_mut().wasm_stream_info = wasm_stream_info;

    let ret: Result<()> = async {
        for plugin_handle_serverless in &*net_core_plugin_conf.plugin_handle_serverless.get().await
        {
            let err = (plugin_handle_serverless)(stream_info.clone()).await?;
            match err {
                crate::Error::Ok => {}
                crate::Error::Break => {}
                crate::Error::Finish => {
                    break;
                }
                crate::Error::Error => {
                    return Err(anyhow::anyhow!("err:plugin_handle_serverless"));
                }
                crate::Error::Return => {
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    ret?;
    stream_info
        .get_mut()
        .wasm_stream_info_map
        .get_mut()
        .remove(&session_id);
    Ok(true)
}

pub async fn run_plugin_handle_http(
    r: &Arc<HttpStreamRequest>,
    scc: &Arc<StreamConfigContext>,
) -> Result<()> {
    let plugin_handle_access = {
        use crate::config::net_core_plugin;
        //___wait___
        //let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_core_conf());
        let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());
        net_core_plugin_conf.plugin_handle_http.clone()
    };

    for plugin_handle_access in &*plugin_handle_access.get().await {
        let err = (plugin_handle_access)(r.clone()).await?;
        match err {
            crate::Error::Ok => {}
            crate::Error::Break => {}
            crate::Error::Finish => {
                break;
            }
            crate::Error::Error => {
                return Err(anyhow::anyhow!("err:plugin_handle_access"));
            }
            crate::Error::Return => {
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

pub async fn run_plugin_handle_websocket(
    scc: &Arc<StreamConfigContext>,
    arg: &ServerArg,
    client_stream: WebSocketStream<BufStream<StreamFlow>>,
) -> Result<bool> {
    let client_stream = ArcMutex::new(client_stream);
    let plugin_handle_access = {
        use crate::config::net_core_plugin;
        //___wait___
        //let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_core_conf());
        let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());
        net_core_plugin_conf.plugin_handle_websocket.clone()
    };

    for plugin_handle_access in &*plugin_handle_access.get().await {
        let err = (plugin_handle_access)(arg.clone(), client_stream.clone()).await?;
        match err {
            crate::Error::Ok => {}
            crate::Error::Break => {}
            crate::Error::Finish => {
                break;
            }
            crate::Error::Error => {
                return Err(anyhow::anyhow!("err:plugin_handle_access"));
            }
            crate::Error::Return => {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub async fn http_serverless(r: &Arc<HttpStreamRequest>) -> Result<()> {
    let wasm_response = r.ctx.get_mut().wasm_response.take();
    if wasm_response.is_none() {
        return Ok(());
    }
    let header_response = r.ctx.get().header_response.clone();
    if header_response.is_none() {
        return Err(anyhow!("err: header_response.is_none()"));
    }
    if header_response.is_send() {
        return Err(anyhow!("err: header_response.is_send()"));
    }

    let (parts, body) = wasm_response.unwrap().into_parts();
    use crate::proxy::http_proxy::http_server_proxy;
    http_server_proxy::HttpStream::stream_response(
        &r,
        HttpResponse {
            response: Response::from_parts(parts, Body::empty()),
            body: HttpResponseBody::Body(body),
        },
    )
    .await?;
    return Ok(());
}

pub fn find_local(stream_info: &Share<StreamInfo>) -> Result<()> {
    let scc = stream_info.get().scc.clone();
    use crate::config::net_local_core;
    let net_local_core_conf = net_local_core::curr_conf(&scc.net_curr_conf());

    let mut local: Option<ArcUnsafeAny> = None;

    for local_rules in &net_local_core_conf.local_rules {
        let stream_info = &mut *stream_info.get_mut();
        let mut data_format_vars = Var::copy(&local_rules.data_format_vars)
            .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
        data_format_vars.for_each(|var| {
            let var_name = Var::var_name(var);
            let value = stream_var::find(var_name, &stream_info);
            match value {
                Err(e) => {
                    log::error!("{}", anyhow!("{}", e));
                    Ok(None)
                }
                Ok(value) => Ok(value),
            }
        })?;

        let data = data_format_vars
            .join()
            .map_err(|e| anyhow!("err:data_format_vars.join => e:{}", e))?;
        log::debug!(target: "main", "data:{}", data);
        if local_rules.regex.is_none() {
            let index = data.find(&local_rules.rule.filter);
            if index.is_none() {
                continue;
            }
            let index = index.unwrap();
            if index != 0 {
                continue;
            }
            local = Some(local_rules.local.clone());
            break;
        }

        let caps = local_rules.regex.captures(&local_rules.rule.filter);
        if caps.is_none() {
            continue;
        } else {
            let caps = caps.map(|cap| {
                cap.iter()
                    .filter_map(|m| m.map(|mat| mat.as_str().to_string()))
                    .collect::<Vec<String>>()
            });

            if caps.is_some() {
                stream_info.caps = caps.into();
            }

            local = Some(local_rules.local.clone());
            break;
        }
    }

    let local = if local.is_none() {
        net_local_core_conf.full_match_local_rule.local.clone()
    } else {
        local.unwrap()
    };

    let ssc = Arc::new(StreamConfigContext {
        ms: scc.ms.clone(),
        net_confs: scc.net_confs.clone(),
        server_confs: scc.server_confs.clone(),
        curr_conf: local,
        common_conf: scc.common_conf.clone(),
    });
    stream_info.get_mut().scc = Some(ssc.clone()).into();
    Ok(())
}

pub async fn rewrite(
    r: &Arc<HttpStreamRequest>,
    scc: &Arc<StreamConfigContext>,
    stream_info: &Share<StreamInfo>,
    header_response: &Arc<HttpHeaderResponse>,
) -> Result<bool> {
    use crate::config::net_core_proxy;
    let net_core_proxy_conf = net_core_proxy::curr_conf(scc.net_curr_conf());
    for v in net_core_proxy_conf.rewrite.iter() {
        let data = v.start(r, stream_info);
        if data.is_err() {
            continue;
        }
        let data = data.unwrap();
        if data.is_none() {
            continue;
        }
        let (url, status) = data.unwrap();

        let uri = url.parse::<http::Uri>();
        let url = if uri.is_err() {
            let ctx = &*r.ctx.get();
            format!(
                "{}://{}:{}{}",
                ctx.r_in.uri.scheme_str().unwrap(),
                ctx.r_in.uri.host().unwrap(),
                ctx.r_in.uri.port_u16().unwrap(),
                url,
            )
        } else {
            url
        };

        let mut response = Response::new(Body::empty());
        *response.status_mut() = status;
        response.headers_mut().insert(
            http::header::LOCATION,
            http::HeaderValue::from_bytes(url.as_bytes())?,
        );

        if !header_response.is_send() {
            use super::http_proxy::http_server_proxy;
            http_server_proxy::HttpStream::stream_response(
                &r,
                HttpResponse {
                    response,
                    body: HttpResponseBody::Body(Body::empty()),
                },
            )
            .await?;
        }
        return Ok(true);
    }

    return Ok(false);
}
