use super::http_hyper_connector::HttpHyperConnector;
use super::HyperExecutorLocal;
use crate::config::config_toml::HttpServerProxyConfig;
use crate::config::config_toml::HttpVersion;
use crate::proxy::http_proxy::stream::Stream;
use crate::proxy::http_proxy::{http_connection, HTTP_HELLO_KEY};
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::stream::connect::{Connect, ConnectInfo};
use any_base::stream_flow::StreamFlow;
use anyhow::anyhow;
use anyhow::Result;
//use hyper::body::HttpBody;
use crate::proxy::http_proxy::http_stream::HttpStream;
use crate::proxy::stream_info::StreamInfo;
use crate::{Protocol7, Protocol77};
use any_base::io::buf_reader::BufReader;
use any_base::typ::{Share, ShareRw};
use base64::{engine::general_purpose, Engine as _};
use http::header::HOST;
use hyper::client::connect::ReqArg;
use hyper::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use hyper::{Body, Version};
use std::sync::Arc;
use std::time::Duration;

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
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::default())?);
        }
        ret
    }

    pub async fn do_run(
        &mut self,
        scc: ShareRw<StreamConfigContext>,
        mut req: Request<Body>,
    ) -> Result<Response<Body>> {
        log::trace!("client req = {:#?}", req);
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
            self.http_arg
                .stream_info
                .get_mut()
                .upstream_protocol_hello_size = hello_str.len();
            req.headers_mut().insert(
                HeaderName::from_bytes(HTTP_HELLO_KEY.as_bytes())?,
                HeaderValue::from_bytes(hello_str.as_bytes())?,
            );
        }

        let socket_fd = {
            let stream_info = self.http_arg.stream_info.get();
            stream_info.server_stream_info.raw_fd
        };

        let version = req.version();
        let protocol7 = connect_func.protocol7().await;
        let upstream_is_tls = connect_func.is_tls().await;

        //let upstream_host = connect_func.host().await?;
        let upstream_host = req.headers().get(HOST);
        if upstream_host.is_none() {
            return Err(anyhow!("host nil"));
        }
        let upstream_host = upstream_host.unwrap().to_str().unwrap();

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

        match upstream_version {
            Version::HTTP_2 => {
                self.http_arg.stream_info.get_mut().upstream_protocol77 = Some(Protocol77::Http2);
            }
            _ => {
                self.http_arg.stream_info.get_mut().upstream_protocol77 = Some(Protocol77::Http);
            }
        }

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
        req.headers_mut().remove(HOST);
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

        let upstream_connect_flow_info = self
            .http_arg
            .stream_info
            .get_mut()
            .upstream_connect_flow_info
            .clone();
        let client_res = client
            .request(
                client_req,
                Some(ReqArg {
                    upstream_connect_flow_info,
                }),
            )
            .await
            .map_err(|e| {
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
            Some(format!("{}:{}", file!(), line!())),
            move |_| async move {
                let is_single = match version {
                    Version::HTTP_2 => false,
                    Version::HTTP_3 => false,
                    _ => true,
                };

                let is_client_sendfile = match version {
                    Version::HTTP_2 => false,
                    Version::HTTP_3 => false,
                    _ => true,
                };

                let is_upstream_sendfile = match upstream_version {
                    Version::HTTP_2 => false,
                    Version::HTTP_3 => false,
                    _ => true,
                };

                let is_protocol7_sendfile = match Protocol7::from_string(&protocol7)? {
                    Protocol7::Tcp => true,
                    _ => false,
                };
                let client_stream_fd = if is_client_sendfile && socket_fd > 0 {
                    1
                } else {
                    0
                };
                let upstream_stream_fd = if is_upstream_sendfile && is_protocol7_sendfile {
                    1
                } else {
                    0
                };

                use crate::proxy::stream_stream::StreamStream;
                let (mut _is_client_sendfile, mut _is_upstream_sendfile) =
                    StreamStream::is_sendfile(
                        client_stream_fd,
                        upstream_stream_fd,
                        scc.clone(),
                        http_arg.stream_info.clone(),
                    );

                let client_stream = Stream::new(req_body, res_sender);
                let client_stream = StreamFlow::new(client_stream, None);

                let upstream_stream = Stream::new(client_res_body, client_req_sender);
                let upstream_stream = StreamFlow::new(upstream_stream, None);

                HttpStream::new(
                    http_arg,
                    scc,
                    upstream_stream,
                    is_single,
                    _is_client_sendfile,
                    _is_upstream_sendfile,
                )
                .start(client_stream)
                .await
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
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&self.http_arg.ms).await;
        let http = HttpHyperConnector::new(
            request_id,
            connect_func,
            common_core_conf.session_id.clone(),
            self.http_arg.executors.context.run_time.clone(),
        );

        let client = hyper::Client::builder()
            .executor(HyperExecutorLocal(self.http_arg.executors.clone()))
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
