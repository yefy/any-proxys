pub mod bitmap;
pub mod http_cache_file;
pub mod http_cache_file_node;
pub mod http_client;
pub mod http_context;
pub mod http_filter;
pub mod http_header_parse;
pub mod http_hyper_connector;
pub mod http_hyper_stream;
pub mod http_in;
pub mod http_server_echo;
pub mod http_server_proxy;
pub mod http_server_purge;
pub mod http_server_static;
pub mod http_server_static_test;
pub mod http_stream_request;
pub mod stream;
pub mod stream_write;
pub mod util;

use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::http_proxy::http_header_parse::parse_request_parts;
use crate::proxy::http_proxy::http_stream_request::{
    HttpResponse, HttpResponseBody, HttpStreamRequest, HttpUpstreamConnectInfo,
};
use crate::proxy::proxy;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_start;
use crate::proxy::util as proxy_util;
use crate::proxy::util::{
    find_local, http_serverless, rewrite, run_plugin_handle_access, run_plugin_handle_http,
    run_plugin_handle_http_serverless,
};
use crate::proxy::ServerArg;
use crate::Protocol77;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::stream_flow::StreamReadWriteFlow;
use any_base::typ::Share;
use any_base::util::{ArcString, HttpHeaderExt};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use http::header::HOST;
use hyper::http::StatusCode;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::Version;
use hyper::{Body, Request, Response};
use lazy_static::lazy_static;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

/*
is_request_cache == false
      动态回源
           没长度    transfer_encoding    content_length == 0
           有长度
r_ctx.r_in.curr_slice_start
r_ctx.r_out.transfer_encoding
ctx.r_out.left_content_length
rctx.r_out.is_cache_err = false;
 rctx.r_out.is_cache


content_length == 0  不走body流程

request头没有CACHE_CONTROL
500错误
echo

500错误响应需要修改文件头， 其它不需要去修改




is_request_cache == true
     不缓存
           没长度     transfer_encoding    content_length == 0
           有长度   需要校验
     缓存
          有长度   需要校验

content_length == 0  不走body流程

静态服务器   文件缓存  500错误
*/

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

use crate::proxy::stream_stream::StreamStream;
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    arg.stream_info.get_mut().add_work_time1("http");
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );

    http_connection(arg, client_buf_stream).await
}

pub async fn http_connection<RW: StreamReadWriteFlow + Unpin + Send + 'static>(
    arg: ServerArg,
    stream: RW,
) -> Result<()> {
    arg.stream_info.get_mut().is_discard_flow = true;
    arg.stream_info.get_mut().is_discard_timeout = true;
    arg.stream_info.get_mut().is_discard_access_log = true;
    let mut shutdown_thread_rx = arg.executors.context.shutdown_thread_tx.subscribe();
    arg.executors.clone()._start(
        #[cfg(feature = "anyspawn-count")]
        Some(format!("{}:{}", file!(), line!())),
        move |_| async move {
            log::debug!(target: "ext3", "Http::new");
            let conn = Http::new()
                .with_executor(HyperExecutorLocal(arg.executors.clone()))
                .http2_only(false)
                .http1_keep_alive(true)
                .serve_connection(
                    stream,
                    service_fn(move |request| http_handle(0, arg.clone(), request, None)),
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
    r: &Arc<HttpStreamRequest>,
    arg: ServerArg,
    mut request: Request<Body>,
    upstream_connect_info: Option<Arc<HttpUpstreamConnectInfo>>,
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
            hyper::AnyProxyHyperHttpHeaderExt(HttpHeaderExt::new(Arc::new(AtomicI64::new(0)))),
        ));
    log::debug!(target: "ext3",
                "r.session_id:{}-{}, http_handle_local request:{:#?}",
        r.session_id,
        r.local_cache_req_count,
        request
    );
    let response = http_handle(r.session_id, arg, request, upstream_connect_info).await?;
    log::debug!(target: "ext3",
                "r.session_id:{}-{}, http_handle_local response:{:#?}",
                r.session_id,
                r.local_cache_req_count,
                response
    );
    Ok(response)
}

pub async fn http_handle(
    _session_id: u64,
    arg: ServerArg,
    request: Request<Body>,
    upstream_connect_info: Option<Arc<HttpUpstreamConnectInfo>>,
) -> Result<Response<Body>> {
    use crate::config::net_core;
    let net_core_conf = net_core::main_conf(&arg.ms).await;

    use crate::config::common_core;
    let common_core_conf = common_core::main_conf_mut(&arg.ms).await;
    let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
    if _session_id > 0 {
        log::debug!(target: "ext3",
                    "r.session_id:{}, http_handle_local, {} to {}",
                    session_id,
                    _session_id,
                    session_id,
        );
    }

    let wasm_stream_info_map = {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf_mut(&arg.ms).await;
        net_core_wasm_conf.wasm_stream_info_map.clone()
    };

    let mut stream_info = StreamInfo::new(
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

    let domain_config_context = arg
        .domain_config_listen
        .domain_config_context_map
        .iter()
        .last();
    if domain_config_context.is_some() {
        let (_, domain_config_context) = domain_config_context.unwrap();
        stream_info.scc = Some(domain_config_context.scc.to_arc_scc(arg.ms.clone())).into();
    }

    let version = request.version();
    match version {
        Version::HTTP_2 => {
            stream_info.client_protocol77 = Some(Protocol77::Http2);
        }
        _ => {
            stream_info.client_protocol77 = Some(Protocol77::Http);
        }
    }
    let stream_info = Share::new(stream_info);

    let http_arg = ServerArg {
        ms: arg.ms.clone(),
        executors: arg.executors.clone(),
        stream_info: stream_info.clone(),
        domain_config_listen: arg.domain_config_listen.clone(),
        server_stream_info: arg.server_stream_info.clone(),
        http_context: arg.http_context.clone(),
    };

    let (tx, rx) = async_channel::bounded(10);
    http_arg.executors.clone()._start(
        #[cfg(feature = "anyspawn-count")]
        Some(format!("{}:{}", file!(), line!())),
        move |_| async move {
            HttpStream::new(arg, http_arg, request, tx, upstream_connect_info)
                .start()
                .await
        },
    );
    Ok(rx
        .recv()
        .await
        .map_err(|e| anyhow::anyhow!("err:http_server_run_handle Response<Body> => err:{}", e))?)
}

pub struct HttpStream {
    pub arg: ServerArg,
    pub http_arg: ServerArg,
    pub request: Option<Request<Body>>,
    pub header_response: Arc<HttpHeaderResponse>,
    pub upstream_connect_info: Option<Arc<HttpUpstreamConnectInfo>>,
}

impl HttpStream {
    pub fn new(
        arg: ServerArg,
        http_arg: ServerArg,
        request: Request<Body>,
        response_tx: async_channel::Sender<Response<Body>>,
        upstream_connect_info: Option<Arc<HttpUpstreamConnectInfo>>,
    ) -> HttpStream {
        let header_response = Arc::new(HttpHeaderResponse::new(response_tx));
        HttpStream {
            arg,
            http_arg,
            request: Some(request),
            header_response,
            upstream_connect_info,
        }
    }

    pub async fn start(self) -> Result<()> {
        let stream_info = self.http_arg.stream_info.clone();
        let shutdown_thread_rx = self
            .http_arg
            .executors
            .context
            .shutdown_thread_tx
            .subscribe();
        let header_response = self.header_response.clone();
        let ret = stream_start::do_start(self, &stream_info, shutdown_thread_rx).await;
        stream_info.get_mut().http_r.set_nil();
        if ret.is_err() {
            let _ = stream_send_err_head(header_response).await;
        }
        ret
    }
    pub fn request_parse(
        &self,
        r: &Arc<HttpStreamRequest>,
        stream_info: &Share<StreamInfo>,
        version: Version,
    ) -> Result<()> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        use crate::config::net_core_proxy;
        let net_core_proxy_conf = net_core_proxy::curr_conf(scc.net_curr_conf());
        let http_request_slice = net_core_proxy_conf.proxy_request_slice;

        let socket_fd = {
            let stream_info = stream_info.get();
            stream_info.server_stream_info.raw_fd
        };

        let is_client_sendfile = match version {
            Version::HTTP_2 => false,
            Version::HTTP_3 => false,
            _ => true,
        };

        let client_stream_fd = if is_client_sendfile && socket_fd > 0 {
            1
        } else {
            0
        };

        let mut is_client_sendfile =
            StreamStream::is_client_sendfile(client_stream_fd, &scc, &stream_info);

        if r.local_cache_req_count > 0 {
            is_client_sendfile = false;
        }

        let rctx = &mut *r.ctx.get_mut();
        if !rctx.r_in.is_range
            && net_core_proxy_conf.proxy_get_to_get_range
            && (rctx.r_in.is_get || rctx.r_in.is_head)
        {
            rctx.r_in.is_range = true;
        }
        rctx.http_request_slice = http_request_slice;
        rctx.is_client_sendfile = is_client_sendfile;

        Ok(())
    }

    pub async fn run(
        &self,
        r: &Arc<HttpStreamRequest>,
        stream_info: &Share<StreamInfo>,
        version: Version,
    ) -> Result<()> {
        let (domain, hello_str) = {
            let r_in = &mut r.ctx.get_mut().r_in;
            let domain = ArcString::new(r_in.uri.host().unwrap().to_string());
            let hello_str = r_in.upstream_headers.remove(&*HTTP_HELLO_KEY);
            (domain, hello_str)
        };

        let scc = proxy_util::parse_proxy_domain(
            &self.http_arg,
            move || async move {
                let hello = {
                    if hello_str.is_none() {
                        None
                    } else {
                        let hello = general_purpose::STANDARD
                            .decode(hello_str.as_ref().unwrap().as_bytes())?;
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

        stream_info.get_mut().err_status = ErrStatus::AccessLimit;
        if run_plugin_handle_access(&scc, &stream_info).await? {
            return Err(anyhow!("access"));
        }

        stream_info.get_mut().err_status = ErrStatus::ServerErr;
        if rewrite(&r, &scc, &stream_info, &self.header_response).await? {
            return Ok(());
        }

        find_local(&stream_info)?;
        let scc = stream_info.get().scc.clone();

        if rewrite(&r, &scc, &stream_info, &self.header_response).await? {
            return Ok(());
        }

        self.request_parse(&r, &stream_info, version)?;
        let ms = scc.ms().clone();
        use crate::config::net_core_plugin;
        let http_core_plugin_main_conf = net_core_plugin::main_conf(&ms).await;
        let plugin_http_header_in = http_core_plugin_main_conf.plugin_http_header_in.get().await;
        (plugin_http_header_in)(r.clone()).await?;

        if run_plugin_handle_http_serverless(&scc, &stream_info).await? {
            return http_serverless(r).await;
        }

        run_plugin_handle_http(&r, &scc).await
    }
}

#[async_trait]
impl proxy::Stream for HttpStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>) -> Result<()> {
        stream_info.get_mut().err_status = ErrStatus::ClientProtoErr;
        stream_info
            .get_mut()
            .add_work_time1("net_server_http_proxy");

        let session_id = self.http_arg.stream_info.get().session_id;
        let mut request = self.request.take().unwrap();
        log::debug!(target: "ext3", "r.session_id:{}, client headers = {:#?}", session_id, request);
        let (req_host, part) = parse_request_parts(&stream_info, &request)?;
        if req_host.is_some() {
            request.headers_mut().insert(
                HOST,
                http::HeaderValue::from_bytes(req_host.unwrap().as_bytes())?,
            );
        }
        let version = request.version();
        let r = Arc::new(
            HttpStreamRequest::new(
                self.arg.clone(),
                self.http_arg.clone(),
                session_id,
                Some(self.header_response.clone()).into(),
                part,
                request,
                self.upstream_connect_info.clone(),
            )
            .await?,
        );
        r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();

        self.request_parse(&r, &stream_info, version)?;

        let ret = self.run(&r, &stream_info, version).await;
        if let Err(e) = ret {
            log::debug!(target: "ext3",
                        "r.session_id:{}-{}, err:run => e:{}",
                        r.session_id,
                        r.local_cache_req_count,
                        e
            );

            if !self.header_response.is_send() {
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::default())?;
                log::debug!(target: "ext3",
                            "r.session_id:{}-{}, err:run response:{:#?}",
                            r.session_id,
                            r.local_cache_req_count,
                            response
                );
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
        }

        http_server_proxy::HttpStream::stream_end_err(&r).await?;
        http_server_proxy::HttpStream::stream_end_free(&r).await?;
        http_server_proxy::HttpStream::cache_file_node_to_pool(&r).await?;
        Ok(())
    }
}
