use super::http_hyper_connector::HttpHyperConnector;
use super::util;
use super::HyperExecutorLocal;
use crate::config::config_toml::HttpServerProxyConfig;
use crate::config::config_toml::HttpVersion;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::http_proxy::stream::Stream;
use crate::proxy::http_proxy::{Handle, HTTP_HELLO_KEY};
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::stream::connect::{Connect, ConnectInfo};
use crate::util::util::host_and_port;
use any_base::stream_flow::{StreamFlow, StreamFlowInfo};
use anyhow::anyhow;
use anyhow::Result;
//use hyper::body::HttpBody;
use crate::proxy::http_proxy::http_stream::HttpStream;
use crate::proxy::stream_info::StreamInfo;
use crate::{Protocol7, Protocol77};
use any_base::executor_local_spawn::ThreadRuntime;
use any_base::io::buf_reader::BufReader;
use any_base::typ::{ArcMutex, Share, ShareRw};
use base64::{engine::general_purpose, Engine as _};
use hyper::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Version};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

pub async fn http_connection<RW: AsyncRead + AsyncWrite + Unpin + 'static>(
    arg: ServerArg,
    stream: RW,
    handle: Handle,
) -> Result<()> {
    arg.stream_info.get_mut().is_discard_flow = true;
    arg.stream_info.get_mut().is_discard_timeout = true;
    let conn = Http::new()
        .with_executor(HyperExecutorLocal(arg.executors.clone()))
        .http2_only(false)
        .http1_keep_alive(true)
        .serve_connection(
            stream,
            service_fn(move |req| http_handle(arg.clone(), req, handle.clone())),
        );
    let _ = conn.await;
    // if let Err(e) = conn.await {
    //     if !(e.is_closed() || e.is_canceled()) {
    //         return Err(anyhow!("err:http_connection => e:{}", e));
    //     }
    // }
    Ok(())
}

pub async fn http_handle(
    arg: ServerArg,
    req: Request<Body>,
    handle: Handle,
) -> Result<Response<Body>> {
    let mut shutdown_thread_rx = arg.executors.shutdown_thread_tx.subscribe();
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
    let domain = domain.to_string();
    let hello_str = req.headers().get(&*HTTP_HELLO_KEY).cloned();

    use crate::config::http_core;
    let http_core_conf = http_core::main_conf(&arg.ms).await;

    let stream_info = StreamInfo::new(
        arg.server_stream_info.clone(),
        http_core_conf.debug_is_open_stream_work_times,
        Some(arg.executors.clone()),
    );
    let stream_info = Share::new(stream_info);
    stream_info.get_mut().protocol77 = Some(Protocol77::Http);

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

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );
    http_connection(arg, client_buf_stream, |arg, http_arg, scc, req| {
        Box::pin(http_server_run_handle(arg, http_arg, scc, req))
    })
    .await
}

pub async fn http_server_run_handle(
    _arg: ServerArg,
    http_arg: ServerArg,
    scc: ShareRw<StreamConfigContext>,
    req: Request<Body>,
) -> Result<Response<Body>> {
    HttpServer::new(http_arg).run(scc, req).await
}

pub struct HttpServer {
    http_arg: ServerArg,
}

impl HttpServer {
    pub fn new(http_arg: ServerArg) -> HttpServer {
        HttpServer { http_arg }
    }

    pub async fn run(
        &mut self,
        scc: ShareRw<StreamConfigContext>,
        req: Request<Body>,
    ) -> Result<Response<Body>> {
        let ret = self.do_run(scc, req).await;
        if let Err(e) = ret {
            log::error!("err:run => e:{}", e);
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::default())?);
        }
        ret
    }

    pub async fn do_run(
        &mut self,
        scc: ShareRw<StreamConfigContext>,
        mut req: Request<Body>,
    ) -> Result<Response<Body>> {
        let (is_proxy_protocol_hello, connect_func) =
            proxy_util::upsteam_connect_info(self.http_arg.stream_info.clone(), scc.clone())
                .await?;

        let hello = proxy_util::get_proxy_hello(
            is_proxy_protocol_hello,
            self.http_arg.stream_info.clone(),
            scc.clone(),
        )
        .await;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            let hello_str = general_purpose::STANDARD.encode(hello_str);
            self.http_arg.stream_info.get_mut().protocol_hello_size = hello_str.len();
            req.headers_mut().insert(
                HeaderName::from_bytes(HTTP_HELLO_KEY.as_bytes())?,
                HeaderValue::from_bytes(hello_str.as_bytes())?,
            );
        }

        let protocol7 = connect_func.protocol7().await;
        let upstream_host = connect_func.host().await?;
        let upstream_is_tls = connect_func.is_tls().await;

        let version = req.version();
        log::trace!("client req = {:#?}", req);

        let proxy = {
            let scc = scc.get();
            use crate::config::http_server_proxy;
            let http_server_proxy_conf = http_server_proxy::currs_conf(scc.http_server_confs());
            http_server_proxy_conf.proxy.clone()
        };

        let upstream_version = match proxy.proxy_pass.version {
            HttpVersion::Http1_1 => Version::HTTP_11,
            HttpVersion::Http2_0 => Version::HTTP_2,
            HttpVersion::Auto => match version {
                Version::HTTP_2 => Version::HTTP_2,
                _ => Version::HTTP_11,
            },
        };

        let upstream_scheme = if upstream_is_tls { "https" } else { "http" };
        let url_string = format!(
            "{}://{}{}",
            upstream_scheme,
            upstream_host,
            req.uri()
                .path_and_query()
                .map(|x| x.as_str())
                .unwrap_or("/")
        );
        log::trace!("url_string = {}", url_string);
        let uri = url_string.parse()?;

        *req.uri_mut() = uri;
        req.headers_mut().remove("connection");
        *req.version_mut() = upstream_version;
        log::trace!("upstream req = {:#?}", req);

        let (req_parts, req_body) = req.into_parts();
        let (client_req_sender, client_req_body) = Body::channel();
        let client_req = Request::from_parts(req_parts, client_req_body);

        let client = self
            .get_client(
                upstream_version,
                connect_func.clone(),
                &proxy,
                self.http_arg.stream_info.clone(),
                &protocol7,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "err:get_client => request_id:{}, protocol7:{}, e:{}",
                    self.http_arg.stream_info.get().request_id,
                    protocol7,
                    e
                )
            })?;

        let client_res = client.request(client_req).await.map_err(|e| {
            anyhow!(
                "err:client.request => request_id:{}, protocol7:{}, e:{}",
                self.http_arg.stream_info.get().request_id,
                protocol7,
                e
            )
        })?;

        let (client_res_parts, client_res_body) = client_res.into_parts();
        let (res_sender, res_body) = Body::channel();
        let mut resp = Response::from_parts(client_res_parts, res_body);

        if version == Version::HTTP_10 {
            resp.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        let http_arg = self.http_arg.clone();
        self.http_arg.executors._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
            move |_| async move {
                let client_stream = Stream::new(req_body, res_sender);
                let (r, w) = any_base::io::split::split(client_stream);
                let client_stream =
                    StreamFlow::new(0, ArcMutex::new(Box::new(r)), ArcMutex::new(Box::new(w)));

                let upstream_stream = Stream::new(client_res_body, client_req_sender);
                let (r, w) = any_base::io::split::split(upstream_stream);
                let mut upstream_stream =
                    StreamFlow::new(0, ArcMutex::new(Box::new(r)), ArcMutex::new(Box::new(w)));
                upstream_stream
                    .set_stream_info(http_arg.stream_info.get().upstream_stream_flow_info.clone());

                let mut shutdown_thread_rx = http_arg.executors.shutdown_thread_tx.subscribe();
                tokio::select! {
                    biased;
                    ret = HttpStream::new(http_arg, scc, upstream_stream)
                                .start(client_stream) => {
                        return ret;
                    }
                    _ = shutdown_thread_rx.recv() => {
                        return Err(anyhow!("Internal Server Error"));
                    }
                    else => {
                        return Err(anyhow!("Internal Server Error"));
                    }
                }

                // HttpStream::new(http_arg, scc, upstream_stream)
                //     .start(client_stream)
                //     .await
            },
        );

        Ok(resp)
    }

    pub async fn get_client(
        &self,
        version: Version,
        connect_func: Arc<Box<dyn Connect>>,
        config: &HttpServerProxyConfig,
        stream_info: Share<StreamInfo>,
        protocol7: &str,
    ) -> Result<Arc<hyper::Client<HttpHyperConnector>>> {
        let addr = connect_func.addr().await?;
        let is_http2 = match &version {
            &hyper::http::Version::HTTP_11 => false,
            &hyper::http::Version::HTTP_2 => true,
            _ => {
                return Err(anyhow::anyhow!(
                    "err:http version not found => version:{:?}",
                    version
                ))?
            }
        };
        let upstream_connect_info = ConnectInfo {
            protocol7: Protocol7::from_string(&protocol7)?,
            domain: connect_func.domain().await,
            elapsed: 0.0,
            local_addr: addr.clone(),
            remote_addr: addr.clone(),
            peer_stream_size: None,
            max_stream_size: None,
            min_stream_cache_size: None,
            channel_size: None,
        };
        stream_info
            .get_mut()
            .upstream_connect_info
            .set(upstream_connect_info);

        let http_context = {
            let stream_info = self.http_arg.stream_info.get();
            let scc = stream_info.scc.get();
            use crate::config::http_core;
            let http_core_conf = http_core::currs_conf(scc.http_server_confs());

            if http_core_conf.is_disable_share_http_context {
                self.http_arg.http_context.clone()
            } else {
                http_core_conf.http_context.clone()
            }
        };

        let key = format!("{}-{}-{}", protocol7, addr, is_http2);
        let client = http_context.client_map.get().get(&key).cloned();
        if client.is_some() {
            return Ok(client.unwrap());
        }

        let request_id = self.http_arg.stream_info.get().request_id.clone();
        let upstream_connect_flow_info = ArcMutex::new(StreamFlowInfo::new());
        let upstream_stream_flow_info = ArcMutex::new(StreamFlowInfo::new());
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&self.http_arg.ms).await;
        let http = HttpHyperConnector::new(
            upstream_connect_flow_info,
            request_id,
            upstream_stream_flow_info,
            connect_func,
            common_core_conf.session_id.clone(),
            //___wait___
            //HttpHyperConnector 和 any-tunnel 不能同时使用localspawn， 分别都使用spawn_local和spawn是没问题的
            //spawn_local` called from outside of a `task::LocalSet`, form tunnel peer_stream.rs
            //self.http_arg.executors.run_time.clone(),
            Arc::new(Box::new(ThreadRuntime)),
        );

        let client = hyper::Client::builder()
            .pool_max_idle_per_host(config.proxy_pass.pool_max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.proxy_pass.pool_idle_timeout))
            .http2_only(is_http2)
            //.set_host(false)
            .build(http);
        let client = Arc::new(client);

        http_context
            .client_map
            .get_mut()
            .insert(key, client.clone());

        Ok(client)
    }
}
