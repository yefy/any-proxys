use crate::protopack::anyproxy::{AnyproxyHello, ANYPROXY_VERSION};
use crate::proxy::stream_info::{ErrStatus, StreamInfo};
use crate::proxy::{stream_var, ServerArg, StreamConfigContext};
use crate::stream::connect::Connect;
use any_base::stream_flow::StreamFlow;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub async fn parse_proxy_domain<S, SF, D, DF>(
    arg: &ServerArg,
    hello_service: S,
    domain_service: D,
) -> Result<Rc<StreamConfigContext>>
where
    S: FnOnce() -> SF,
    SF: Future<Output = Result<Option<(AnyproxyHello, usize)>>>,
    D: FnOnce() -> DF,
    DF: Future<Output = Result<String>>,
{
    arg.stream_info.borrow_mut().add_work_time("hello");
    let hello = {
        arg.stream_info.borrow_mut().err_status = ErrStatus::ClientProtoErr;
        let hello = hello_service().await?;
        match hello {
            Some((hello, hello_pack_size)) => {
                arg.stream_info.borrow_mut().client_protocol_hello_size = hello_pack_size;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.borrow_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.borrow_mut().ssl_domain = Some(hello.domain.clone());
                }
                arg.stream_info.borrow_mut().remote_domain = Some(hello.domain.clone());
                arg.stream_info.borrow_mut().local_domain = Some(hello.domain.clone());
                hello
            }
            None => {
                let domain = domain_service().await?;
                if arg.server_stream_info.domain.is_some() {
                    arg.stream_info.borrow_mut().ssl_domain = arg.server_stream_info.domain.clone();
                } else {
                    arg.stream_info.borrow_mut().ssl_domain = Some(domain.clone());
                }
                arg.stream_info.borrow_mut().ssl_domain = Some(domain.clone());
                arg.stream_info.borrow_mut().remote_domain = Some(domain.clone());
                arg.stream_info.borrow_mut().local_domain = Some(domain.clone());

                AnyproxyHello {
                    version: ANYPROXY_VERSION.to_string(),
                    request_id: format!(
                        "{:?}_{}_{}_{}_{}",
                        arg.stream_info.borrow().server_stream_info.local_addr,
                        arg.stream_info.borrow().server_stream_info.remote_addr,
                        unsafe { libc::getpid() },
                        arg.session_id.fetch_add(1, Ordering::Relaxed),
                        Local::now().timestamp_millis()
                    ),
                    client_addr: arg
                        .stream_info
                        .borrow()
                        .server_stream_info
                        .remote_addr
                        .clone(),
                    domain,
                }
            }
        }
    };

    arg.stream_info.borrow_mut().request_id = hello.request_id.clone();
    {
        *arg.stream_info.borrow().protocol_hello.lock().unwrap() = Some(Arc::new(hello));
    }

    let domain_index = {
        arg.domain_config_listen
            .domain_index
            .index(
                &arg.stream_info
                    .borrow()
                    .protocol_hello
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .domain,
            )
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

    let stream_config_context = domain_config_context.stream_config_context.clone();

    arg.stream_info.borrow_mut().stream_config_context = Some(stream_config_context.clone());
    arg.stream_info.borrow_mut().debug_is_open_print =
        stream_config_context.fast_conf.debug_is_open_print;
    arg.stream_info.borrow_mut().debug_is_open_stream_work_times =
        stream_config_context.stream.debug_is_open_stream_work_times;
    Ok(stream_config_context)
}

pub async fn upsteam_connect_info(
    stream_info: Rc<RefCell<StreamInfo>>,
    stream_config_context: &StreamConfigContext,
) -> Result<(Option<bool>, Arc<Box<dyn Connect>>)> {
    let client_ip = stream_var::client_ip(&stream_info.borrow());
    if client_ip.is_none() {
        return Err(anyhow!("err: client_ip nil"));
    }
    let client_ip = client_ip.unwrap();

    stream_info.borrow_mut().err_status = ErrStatus::ServiceUnavailable;
    let (ups_dispatch, connect_info) = {
        stream_config_context
            .ups_data
            .lock()
            .unwrap()
            .get_connect(&client_ip)
    };
    stream_info.borrow_mut().ups_dispatch = Some(ups_dispatch);
    if connect_info.is_none() {
        return Err(anyhow!("err: connect_func nil"));
    }
    let (is_proxy_protocol_hello, connect_func) = connect_info.unwrap();

    Ok((is_proxy_protocol_hello, connect_func))
}

pub async fn upsteam_do_connect(
    stream_info: Rc<RefCell<StreamInfo>>,
    connect_func: Arc<Box<dyn Connect>>,
) -> Result<StreamFlow> {
    let upstream_connect_flow_info = stream_info.borrow().upstream_connect_flow_info.clone();
    let request_id = stream_info.borrow().request_id.clone();
    let connect_info = connect_func
        .connect(
            Some(request_id.clone()),
            &mut Some(&mut *upstream_connect_flow_info.lock().await),
        )
        .await
        .map_err(|e| {
            anyhow!(
                "err:connect => request_id:{}, e:{}",
                stream_info.borrow().request_id,
                e
            )
        })?;

    let (mut upstream_stream, upstream_connect_info) = connect_info;

    log::trace!(
        "skip ebpf warning request_id:{}, server_stream_info:{:?}, ups_local_addr:{}, ups_remote_addr:{}",
        stream_info.borrow().request_id,
        stream_info.borrow().server_stream_info,
        upstream_connect_info.local_addr,
        upstream_connect_info.remote_addr
    );

    log::trace!(
        "upstream_protocol_name:{}",
        upstream_connect_info.protocol7.to_string()
    );

    stream_info.borrow_mut().upstream_connect_info = Some(Rc::new(upstream_connect_info));
    upstream_stream.set_stream_info(Some(stream_info.borrow().upstream_stream_flow_info.clone()));

    Ok(upstream_stream)
}

pub async fn upsteam_connect(
    stream_info: Rc<RefCell<StreamInfo>>,
    stream_config_context: &StreamConfigContext,
) -> Result<(Option<bool>, StreamFlow)> {
    stream_info.borrow_mut().add_work_time("upsteam_connect");

    let (is_proxy_protocol_hello, connect_func) =
        upsteam_connect_info(stream_info.clone(), stream_config_context).await?;

    let upstream_stream = upsteam_do_connect(stream_info, connect_func).await?;

    Ok((is_proxy_protocol_hello, upstream_stream))
}

pub async fn get_proxy_hello(
    is_proxy_protocol_hello: Option<bool>,
    stream_info: Rc<RefCell<StreamInfo>>,
    stream_config_context: &StreamConfigContext,
) -> Option<Arc<AnyproxyHello>> {
    let is_proxy_protocol_hello = {
        let mut stream_info = stream_info.borrow_mut();
        stream_info.err_status = ErrStatus::Ok;

        let is_proxy_protocol_hello = if is_proxy_protocol_hello.is_none() {
            if stream_config_context.is_proxy_protocol_hello.is_none() {
                false
            } else {
                stream_config_context.is_proxy_protocol_hello.unwrap()
            }
        } else {
            is_proxy_protocol_hello.unwrap()
        };
        stream_info.is_proxy_protocol_hello = is_proxy_protocol_hello;
        is_proxy_protocol_hello
    };

    if is_proxy_protocol_hello {
        let hello = {
            stream_info
                .borrow()
                .protocol_hello
                .lock()
                .unwrap()
                .clone()
                .unwrap()
        };
        Some(hello)
    } else {
        None
    }
}
