use crate::protopack::anyproxy::{AnyproxyHello, ANYPROXY_VERSION};
use crate::proxy::stream_info::{ErrStatus, StreamInfo};
use crate::proxy::{stream_var, ServerArg, StreamConfigContext};
use crate::stream::connect::Connect;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{Share, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub async fn parse_proxy_domain<S, SF, D, DF>(
    arg: &ServerArg,
    hello_service: S,
    domain_service: D,
) -> Result<ShareRw<StreamConfigContext>>
where
    S: FnOnce() -> SF,
    SF: Future<Output = Result<Option<(AnyproxyHello, usize)>>>,
    D: FnOnce() -> DF,
    DF: Future<Output = Result<String>>,
{
    arg.stream_info.get_mut().add_work_time("hello");
    let hello = {
        arg.stream_info.get_mut().err_status = ErrStatus::ClientProtoErr;
        let hello = hello_service().await?;
        match hello {
            Some((hello, hello_pack_size)) => {
                arg.stream_info.get_mut().client_protocol_hello_size = hello_pack_size;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.get_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.get_mut().ssl_domain = Some(hello.domain.clone());
                }
                arg.stream_info.get_mut().remote_domain = Some(hello.domain.clone());
                arg.stream_info.get_mut().local_domain = Some(hello.domain.clone());
                hello
            }
            None => {
                let domain = domain_service().await?;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.get_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.get_mut().ssl_domain = Some(domain.clone());
                }
                arg.stream_info.get_mut().ssl_domain = Some(domain.clone());
                arg.stream_info.get_mut().remote_domain = Some(domain.clone());
                arg.stream_info.get_mut().local_domain = Some(domain.clone());
                use crate::config::common_core;
                let common_core_conf = common_core::main_conf_mut(&arg.ms).await;

                let server_stream_info = arg.stream_info.get().server_stream_info.clone();
                AnyproxyHello {
                    version: ANYPROXY_VERSION.to_string(),
                    request_id: format!(
                        "{:?}_{}_{}_{}_{}",
                        server_stream_info.local_addr,
                        server_stream_info.remote_addr,
                        unsafe { libc::getpid() },
                        common_core_conf.session_id.fetch_add(1, Ordering::Relaxed),
                        Local::now().timestamp_millis()
                    ),
                    client_addr: server_stream_info.remote_addr.clone(),
                    domain,
                }
            }
        }
    };

    arg.stream_info.get_mut().request_id = hello.request_id.clone();
    {
        arg.stream_info.get().protocol_hello.set(Arc::new(hello));
    }

    let domain_index = {
        arg.domain_config_listen
            .domain_index
            .index(&arg.stream_info.get().protocol_hello.get().domain)
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

    let scc = domain_config_context.scc.clone();
    let (debug_is_open_print, debug_is_open_stream_work_times) = {
        let scc = scc.get();
        use crate::config::http_core;
        let http_core_conf = http_core::currs_conf(&scc.http_server_confs());
        (
            http_core_conf.debug_is_open_print,
            http_core_conf.debug_is_open_stream_work_times,
        )
    };

    arg.stream_info.get_mut().scc = scc.clone();
    arg.stream_info.get_mut().debug_is_open_print = debug_is_open_print;
    arg.stream_info.get_mut().debug_is_open_stream_work_times = debug_is_open_stream_work_times;
    Ok(scc)
}

pub async fn upsteam_connect_info(
    stream_info: Share<StreamInfo>,
    scc: ShareRw<StreamConfigContext>,
) -> Result<(Option<bool>, Arc<Box<dyn Connect>>)> {
    let client_ip = stream_var::client_ip(&stream_info.get());
    if client_ip.is_none() {
        return Err(anyhow!("err: client_ip nil"));
    }
    let client_ip = client_ip.unwrap();
    let scc = scc.get();
    stream_info.get_mut().err_status = ErrStatus::ServiceUnavailable;
    let (ups_dispatch, connect_info) = {
        use crate::config::http_server_core;
        let http_server_core_conf = http_server_core::currs_conf(scc.http_server_confs());
        http_server_core_conf
            .upstream_data
            .get_mut()
            .balancer_and_connect(&client_ip)?
    };
    stream_info.get_mut().ups_dispatch = Some(ups_dispatch);
    if connect_info.is_none() {
        return Err(anyhow!("err: connect_func nil"));
    }
    let (is_proxy_protocol_hello, connect_func) = connect_info.unwrap();

    Ok((is_proxy_protocol_hello, connect_func))
}

pub async fn upsteam_do_connect(
    stream_info: Share<StreamInfo>,
    connect_func: Arc<Box<dyn Connect>>,
) -> Result<StreamFlow> {
    let upstream_connect_flow_info = stream_info.get().upstream_connect_flow_info.clone();
    let request_id = stream_info.get().request_id.clone();
    let executors = stream_info.get().executors.clone().unwrap();
    let connect_info = connect_func
        .connect(
            Some(request_id.clone()),
            upstream_connect_flow_info,
            Some(executors.run_time),
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
        log::trace!(
            "skip ebpf warning request_id:{}, server_stream_info:{:?}, ups_local_addr:{}, ups_remote_addr:{}",
            stream_info.request_id,
            stream_info.server_stream_info,
            upstream_connect_info.local_addr,
            upstream_connect_info.remote_addr
        );
    }

    log::trace!(
        "upstream_protocol_name:{}",
        upstream_connect_info.protocol7.to_string()
    );

    stream_info
        .get_mut()
        .upstream_connect_info
        .set(upstream_connect_info);
    upstream_stream.set_stream_info(stream_info.get().upstream_stream_flow_info.clone());

    Ok(upstream_stream)
}

pub async fn upsteam_connect(
    stream_info: Share<StreamInfo>,
    scc: ShareRw<StreamConfigContext>,
) -> Result<(Option<bool>, StreamFlow)> {
    stream_info.get_mut().add_work_time("upsteam_connect");

    let (is_proxy_protocol_hello, connect_func) =
        upsteam_connect_info(stream_info.clone(), scc).await?;

    let upstream_stream = upsteam_do_connect(stream_info, connect_func).await?;

    Ok((is_proxy_protocol_hello, upstream_stream))
}

pub async fn get_proxy_hello(
    is_proxy_protocol_hello: Option<bool>,
    stream_info: Share<StreamInfo>,
    scc: ShareRw<StreamConfigContext>,
) -> Option<Arc<AnyproxyHello>> {
    let is_proxy_protocol_hello = {
        let mut stream_info = stream_info.get_mut();
        stream_info.err_status = ErrStatus::Ok;
        let scc = scc.get();
        let is_proxy_protocol_hello = if is_proxy_protocol_hello.is_none() {
            scc.http_core_conf().is_proxy_protocol_hello
        } else {
            is_proxy_protocol_hello.unwrap()
        };
        stream_info.is_proxy_protocol_hello = is_proxy_protocol_hello;
        is_proxy_protocol_hello
    };

    if is_proxy_protocol_hello {
        let hello = { stream_info.get().protocol_hello.get().clone() };
        Some(hello)
    } else {
        None
    }
}
