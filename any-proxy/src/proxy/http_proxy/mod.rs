pub mod bitmap;
pub mod http_cache_file;
pub mod http_cache_file_node;
pub mod http_client;
pub mod http_context;
pub mod http_echo_server;
pub mod http_filter;
pub mod http_header_parse;
pub mod http_hyper_connector;
pub mod http_hyper_stream;
pub mod http_in;
pub mod http_server;
pub mod http_static_server;
pub mod http_static_server_test;
pub mod http_stream;
pub mod http_stream_request;
pub mod stream;
pub mod stream_write;
pub mod util;

use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::http_proxy::http_header_parse::get_http_host;
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::util as proxy_util;
use crate::proxy::util::run_plugin_handle_access;
use crate::proxy::{ServerArg, StreamConfigContext};
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::stream_flow::StreamReadWriteFlow;
use any_base::typ::Share;
use any_base::util::{ArcString, HttpHeaderExt};
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use http::header::HOST;
use hyper::http::StatusCode;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::Version;
use hyper::{Body, Request, Response};
use lazy_static::lazy_static;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct HttpHeaderResponse {
    response_tx: async_channel::Sender<Response<Body>>,
    is_send: Arc<AtomicBool>,
}

impl HttpHeaderResponse {
    pub fn new(response_tx: async_channel::Sender<Response<Body>>) -> HttpHeaderResponse {
        HttpHeaderResponse {
            response_tx,
            is_send: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_send(&self) -> bool {
        self.is_send.load(Ordering::SeqCst)
    }

    pub async fn send_header(&self, head: Response<Body>) -> Result<()> {
        self.is_send.store(true, Ordering::SeqCst);
        self.response_tx.send(head).await?;
        Ok(())
    }
}

pub async fn stream_send_err_head(header_response: Arc<HttpHeaderResponse>) -> Result<()> {
    if !header_response.is_send() {
        let _ = header_response
            .send_header(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::default())?,
            )
            .await;
    }
    Ok(())
}

type Handle = fn(
    arg: ServerArg,
    http_arg: ServerArg,
    scc: Arc<StreamConfigContext>,
    request: Request<Body>,
) -> Pin<Box<dyn Future<Output = Result<Response<Body>>> + Send>>;

lazy_static! {
    pub static ref HTTP_HELLO_KEY: String = "http_hello".to_string();
}

//hyper 单线程执行器
#[derive(Clone)]
pub struct HyperExecutorLocal(pub ExecutorsLocal);
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
                    service_fn(move |request| http_handle(arg.clone(), request, handle.clone())),
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

pub async fn http_handle_local(
    r: Arc<HttpStreamRequest>,
    arg: ServerArg,
    mut request: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let authority = request.uri().authority().cloned();
    if authority.is_some() {
        let uri = request
            .uri()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/");
        use std::str::FromStr;
        *request.uri_mut() = http::uri::Uri::from_str(uri)?;

        request.headers_mut().insert(
            HOST,
            http::HeaderValue::from_str(authority.unwrap().as_str())?,
        );
    }
    request
        .extensions_mut()
        .insert(hyper::AnyProxyRawHttpHeaderExt(
            hyper::AnyProxyHyperHttpHeaderExt(HttpHeaderExt::new()),
        ));
    log::debug!(target: "ext3",
                "r.session_id:{}-{}, http_handle_local request:{:#?}",
        r.session_id,
        r.local_cache_req_count,
        request
    );
    http_handle(arg, request, handle).await
}

pub async fn http_handle(
    arg: ServerArg,
    request: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let mut shutdown_thread_rx = arg.executors.context.shutdown_thread_tx.subscribe();
    tokio::select! {
        biased;
        ret = do_http_handle(arg, request, handle) => {
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
    mut request: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let http_host = get_http_host(&mut request)?;
    let (domain, _) = host_and_port(&http_host);
    let domain = ArcString::new(domain.to_string());
    let hello_str = request.headers().get(&*HTTP_HELLO_KEY).cloned();

    use crate::config::net_core;
    let net_core_conf = net_core::main_conf(&arg.ms).await;

    use crate::config::common_core;
    let common_core_conf = common_core::main_conf_mut(&arg.ms).await;
    let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);

    let wasm_stream_info_map = {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf_mut(&arg.ms).await;
        net_core_wasm_conf.wasm_stream_info_map.clone()
    };

    let stream_info = StreamInfo::new(
        arg.server_stream_info.clone(),
        net_core_conf.debug_is_open_stream_work_times,
        Some(arg.executors.clone()),
        net_core_conf.debug_print_access_log_time,
        net_core_conf.debug_print_stream_flow_time,
        net_core_conf.stream_so_singer_time,
        net_core_conf.debug_is_open_print,
        session_id,
        wasm_stream_info_map,
    );

    let stream_info = Share::new(stream_info);
    let version = request.version();
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
        stream_info: stream_info.clone(),
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
        |_| async { Ok(domain) },
    )
    .await?;

    if run_plugin_handle_access(scc.clone(), stream_info.clone()).await? {
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::default())?);
    }

    (handle)(arg, http_arg, scc, request).await
}
