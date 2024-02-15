pub mod http_context;
pub mod http_echo_server;
pub mod http_hyper_connector;
pub mod http_hyper_stream;
pub mod http_server;
pub mod http_static_server;
pub mod http_static_server_test;
pub mod http_stream;
pub mod stream;
pub mod util;

use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::util as proxy_util;
use crate::proxy::{ServerArg, StreamConfigContext};
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::stream_flow::StreamReadWriteFlow;
use any_base::typ::Share;
use any_base::typ::ShareRw;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use hyper::http::StatusCode;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::Version;
use hyper::{Body, Request, Response};
use lazy_static::lazy_static;
use std::future::Future;
use std::pin::Pin;

type Handle = fn(
    arg: ServerArg,
    http_arg: ServerArg,
    scc: ShareRw<StreamConfigContext>,
    req: Request<Body>,
) -> Pin<Box<dyn Future<Output = Result<Response<Body>>> + Send>>;

lazy_static! {
    pub static ref HTTP_HELLO_KEY: String = "http_hello".to_string();
}

//hyper 单线程执行器
#[derive(Clone)]
pub struct HyperExecutorLocal(ExecutorsLocal);
impl<F> hyper::rt::Executor<F> for HyperExecutorLocal
where
    F: Future<Output = ()> + 'static + Send,
{
    fn execute(&self, fut: F) {
        self.0._start_and_free(
            //#[cfg(feature = "anyspawn-count")]
            // Some(format!("{}:{}", file!(), line!())),
            move |_| async move {
                fut.await;
                Ok(())
            },
        )
    }
}

pub async fn http_connection<RW: StreamReadWriteFlow + Unpin + Send + 'static>(
    arg: ServerArg,
    stream: RW,
    handle: Handle,
) -> Result<()> {
    arg.executors.clone()._start_and_free(
        //#[cfg(feature = "anyspawn-count")]
        //Some(format!("{}:{}", file!(), line!())),
        move |_| async move {
            arg.stream_info.get_mut().is_discard_flow = true;
            arg.stream_info.get_mut().is_discard_timeout = true;
            let mut shutdown_thread_rx = arg.executors.context.shutdown_thread_tx.subscribe();
            let conn = Http::new()
                .with_executor(HyperExecutorLocal(arg.executors.clone()))
                .http2_only(false)
                .http1_keep_alive(true)
                .serve_connection(
                    stream,
                    service_fn(move |req| http_handle(arg.clone(), req, handle.clone())),
                );
            //let _ = conn.await;
            // if let Err(e) = conn.await {
            //     if !(e.is_closed() || e.is_canceled()) {
            //         return Err(anyhow!("err:http_connection => e:{}", e));
            //     }
            // }
            tokio::select! {
                biased;
                _ = conn => {
                }
                _ = shutdown_thread_rx.recv() => {
                }
                else => {
                }
            }
            Ok(())
        },
    );
    Ok(())
}

pub async fn http_handle(
    arg: ServerArg,
    req: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let mut shutdown_thread_rx = arg.executors.context.shutdown_thread_tx.subscribe();
    tokio::select! {
        biased;
        ret = do_http_handle(arg, req, handle) => {
            return ret;
        }
        _ = shutdown_thread_rx.recv() => {
            return Ok(Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::default())?);
        }
        else => {
            return Ok(Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::default())?);
        }
    }
}

pub async fn do_http_handle(
    arg: ServerArg,
    mut req: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let http_host = util::get_http_host(&mut req)?;
    let (domain, _) = host_and_port(&http_host);
    let domain = ArcString::new(domain.to_string());
    let hello_str = req.headers().get(&*HTTP_HELLO_KEY).cloned();

    use crate::config::http_core;
    let http_core_conf = http_core::main_conf(&arg.ms).await;

    let stream_info = StreamInfo::new(
        arg.server_stream_info.clone(),
        http_core_conf.debug_is_open_stream_work_times,
        Some(arg.executors.clone()),
        http_core_conf.debug_print_access_log_time,
        http_core_conf.debug_print_stream_flow_time,
        http_core_conf.stream_so_singer_time,
        http_core_conf.debug_is_open_print,
    );
    let stream_info = Share::new(stream_info);
    let version = req.version();
    match version {
        Version::HTTP_2 => {
            stream_info.get_mut().client_protocol77 = Some(Protocol77::Http2);
        }
        _ => {
            stream_info.get_mut().client_protocol77 = Some(Protocol77::Http);
        }
    }

    let http_arg = ServerArg {
        ms: arg.ms.clone(),
        executors: arg.executors.clone(),
        stream_info,
        domain_config_listen: arg.domain_config_listen.clone(),
        server_stream_info: arg.server_stream_info.clone(),
        http_context: arg.http_context.clone(),
    };

    let scc = proxy_util::parse_proxy_domain(
        &http_arg,
        move || async move {
            let hello = {
                if hello_str.is_none() {
                    None
                } else {
                    let hello =
                        general_purpose::STANDARD.decode(hello_str.as_ref().unwrap().as_bytes())?;
                    let hello: AnyproxyHello = toml::from_slice(&hello)
                        .map_err(|e| anyhow!("err:toml::from_slice=> e:{}", e))?;
                    Some((hello, 0))
                }
            };
            Ok(hello)
        },
        || async { Ok(domain) },
    )
    .await?;

    (handle)(arg, http_arg, scc, req).await
}
