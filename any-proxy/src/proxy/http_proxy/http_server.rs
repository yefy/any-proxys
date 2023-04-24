use super::http_hyper_connector::HttpHyperConnector;
use super::util;
use super::HyperExecutorLocal;
use crate::config::config_toml::HttpVersion;
use crate::config::config_toml::{HttpServerConfig, HttpServerProxyConfig, ServerConfig};
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::http_proxy::http_echo_server::HttpEchoServer;
use crate::proxy::http_proxy::http_static_server::HttpStaticServer;
use crate::proxy::http_proxy::stream::Stream;
use crate::proxy::http_proxy::HTTP_HELLO_KEY;
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
use base64::{engine::general_purpose, Engine as _};
use hyper::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Version};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

pub async fn http_connection<RW: AsyncRead + AsyncWrite + Unpin + 'static>(
    arg: ServerArg,
    stream: RW,
) -> Result<()> {
    arg.stream_info.borrow_mut().is_discard_flow = true;
    arg.stream_info.borrow_mut().is_discard_timeout = true;
    let conn = Http::new()
        .with_executor(HyperExecutorLocal(arg.executors.clone()))
        .http2_only(false)
        .http1_keep_alive(true)
        .serve_connection(stream, service_fn(move |req| handle(arg.clone(), req)));
    let _ = conn.await;
    // if let Err(e) = conn.await {
    //     if !(e.is_closed() || e.is_canceled()) {
    //         return Err(anyhow!("err:http_connection => e:{}", e));
    //     }
    // }
    Ok(())
}

pub async fn handle(arg: ServerArg, req: Request<Body>) -> Result<Response<Body>> {
    let mut shutdown_thread_rx = arg.executors.shutdown_thread_tx.subscribe();
    tokio::select! {
        biased;
        ret = do_handle(arg, req) => {
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

pub async fn do_handle(arg: ServerArg, mut req: Request<Body>) -> Result<Response<Body>> {
    let http_host = util::get_http_host(&mut req)?;
    let (domain, _) = host_and_port(&http_host);
    let domain = domain.to_string();
    let hello_str = req.headers().get(&*HTTP_HELLO_KEY).cloned();

    let stream_info = StreamInfo::new(
        arg.server_stream_info.clone(),
        arg.domain_config_listen
            .stream
            .debug_is_open_stream_work_times,
    );
    let stream_info = Rc::new(RefCell::new(stream_info));
    stream_info.borrow_mut().protocol77 = Some(Protocol77::Http);

    let http_arg = ServerArg {
        executors: arg.executors.clone(),
        stream_info,
        domain_config_listen: arg.domain_config_listen.clone(),
        server_stream_info: arg.server_stream_info.clone(),
        session_id: arg.session_id.clone(),
        http_context: arg.http_context.clone(),
        tmp_file_id: arg.tmp_file_id.clone(),
    };

    let stream_config_context = proxy_util::parse_proxy_domain(
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

    if stream_config_context.server.is_none() {
        return Err(anyhow!("err:stream_config_context.server.is_none"));
    }

    match stream_config_context.clone().server.as_ref().unwrap() {
        &ServerConfig::HttpServer(ref http_server_config) => match &http_server_config.http {
            &HttpServerConfig::EchoServer(ref config) => {
                return HttpEchoServer::new(arg.executors.clone(), req, &config.body)
                    .run()
                    .await;
            }
            &HttpServerConfig::StaticServer(ref config) => {
                return HttpStaticServer::new(arg.executors.clone(), req, &config.path)
                    .run()
                    .await;
            }
            &HttpServerConfig::ProxyServer(ref config) => {
                return HttpServer::new(http_arg)
                    .run(config, stream_config_context, req)
                    .await;
            }
        },
        &ServerConfig::WebsocketServer(_) => {
            return Err(anyhow!("ServerConfig::WebsocketServer"));
        }
    }
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
        config: &HttpServerProxyConfig,
        stream_config_context: Rc<StreamConfigContext>,
        req: Request<Body>,
    ) -> Result<Response<Body>> {
        let ret = self.do_run(config, stream_config_context, req).await;
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
        config: &HttpServerProxyConfig,
        stream_config_context: Rc<StreamConfigContext>,
        mut req: Request<Body>,
    ) -> Result<Response<Body>> {
        let (is_proxy_protocol_hello, connect_func) = proxy_util::upsteam_connect_info(
            self.http_arg.stream_info.clone(),
            &stream_config_context,
        )
        .await?;

        let hello = proxy_util::get_proxy_hello(
            is_proxy_protocol_hello,
            self.http_arg.stream_info.clone(),
            &stream_config_context,
        )
        .await;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            let hello_str = general_purpose::STANDARD.encode(hello_str);
            self.http_arg.stream_info.borrow_mut().protocol_hello_size = hello_str.len();
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

        let upstream_version = match config.proxy_pass.version {
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
                config,
                self.http_arg.stream_info.clone(),
                &protocol7,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "err:get_client => request_id:{}, protocol7:{}, e:{}",
                    self.http_arg.stream_info.borrow().request_id,
                    protocol7,
                    e
                )
            })?;

        let client_res = client.request(client_req).await.map_err(|e| {
            anyhow!(
                "err:client.request => request_id:{}, protocol7:{}, e:{}",
                self.http_arg.stream_info.borrow().request_id,
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
                let client_stream = StreamFlow::new(0, Box::new(r), Box::new(w));

                let upstream_stream = Stream::new(client_res_body, client_req_sender);
                let (r, w) = any_base::io::split::split(upstream_stream);
                let mut upstream_stream = StreamFlow::new(0, Box::new(r), Box::new(w));
                upstream_stream.set_stream_info(Some(
                    http_arg
                        .stream_info
                        .borrow()
                        .upstream_stream_flow_info
                        .clone(),
                ));

                let mut shutdown_thread_rx = http_arg.executors.shutdown_thread_tx.subscribe();
                tokio::select! {
                    biased;
                    ret = HttpStream::new(http_arg, stream_config_context, upstream_stream)
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

                // HttpStream::new(http_arg, stream_config_context, upstream_stream)
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
        stream_info: Rc<RefCell<StreamInfo>>,
        protocol7: &str,
    ) -> Result<Rc<hyper::Client<HttpHyperConnector>>> {
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
        stream_info.borrow_mut().upstream_connect_info = Some(Rc::new(upstream_connect_info));

        let key = format!("{}-{}-{}", protocol7, addr, is_http2);
        let client = self
            .http_arg
            .http_context
            .client_map
            .borrow()
            .get(&key)
            .cloned();
        if client.is_some() {
            return Ok(client.unwrap());
        }

        let request_id = self.http_arg.stream_info.borrow().request_id.clone();
        let upstream_connect_flow_info =
            std::sync::Arc::new(tokio::sync::Mutex::new(StreamFlowInfo::new()));
        let upstream_stream_flow_info =
            std::sync::Arc::new(std::sync::Mutex::new(StreamFlowInfo::new()));

        let http = HttpHyperConnector::new(
            upstream_connect_flow_info,
            request_id,
            upstream_stream_flow_info,
            connect_func,
            self.http_arg.session_id.clone(),
        );

        let client = hyper::Client::builder()
            .pool_max_idle_per_host(config.proxy_pass.pool_max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.proxy_pass.pool_idle_timeout))
            .http2_only(is_http2)
            //.set_host(false)
            .build(http);
        let client = Rc::new(client);

        self.http_arg
            .http_context
            .client_map
            .borrow_mut()
            .insert(key, client.clone());

        Ok(client)
    }
}
