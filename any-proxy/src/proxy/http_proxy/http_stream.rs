use super::http_hyper_connector::HttpHyperConnector;
use crate::config::config_toml::HttpVersion;
use crate::proxy::http_proxy::stream::Stream;
use crate::proxy::http_proxy::{http_handle_local, HTTP_HELLO_KEY};
use crate::proxy::proxy;
use crate::proxy::stream_info::{ErrStatus, StreamInfo};
use crate::proxy::stream_start;
use crate::proxy::util as proxy_util;
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ArcRwLock, ArcRwLockTokio, Share, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
//use hyper::body::HttpBody;
use super::http_cache_file::FILE_CACHE_NODE_MANAGE_ID;
use crate::config::net_core_plugin::PluginHttpFilter;
use crate::config::net_core_proxy;
use crate::proxy::http_proxy::bitmap::align_bitset_start_index;
use crate::proxy::http_proxy::http_cache_file::{
    HttpCacheFile, HttpCacheFileContext, ProxyCacheFileInfo,
};
use crate::proxy::http_proxy::http_cache_file_node::{
    ProxyCacheFileNode, ProxyCacheFileNodeUpstream,
};
use crate::proxy::http_proxy::http_filter::http_filter_header_range::get_http_filter_header_range;
use crate::proxy::http_proxy::http_header_parse::{
    cache_control_time, copy_request_parts, http_headers_size, http_respons_to_vec,
};
use crate::proxy::http_proxy::http_server::http_server_run_handle;
use crate::proxy::http_proxy::http_stream_request::{
    CacheFileStatus, HttpBodyBuf, HttpCacheStatus, HttpRequest, HttpResponse, HttpResponseBody,
    HttpStreamRequest, UpstreamCountDrop, LOCAL_CACHE_REQ_KEY,
};
use crate::proxy::http_proxy::util::{
    bitmap_to_cache_file, del_expires_cache_file, response_body_read, slice_update_bitset,
    update_or_create_cache_file, write_body_to_client, write_cache_file,
};
use crate::proxy::stream_stream::StreamStream;
use crate::proxy::stream_var;
use crate::util::util::host_and_port;
use crate::util::var::{Var, VarAnyData};
use crate::{Protocol7, Protocol77};
use any_base::file_ext::FileCacheBytes;
use any_base::stream_nil_read;
use any_base::stream_nil_write;
use any_base::util::{ArcString, HttpHeaderExt};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use http::header::HOST;
use http::StatusCode;
use hyper::client::connect::ReqArg;
use hyper::http::{HeaderName, HeaderValue, Request, Response};
use hyper::{Body, Version};
use rand::Rng;
use std::collections::VecDeque;
use std::mem::swap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub struct HttpStream {
    pub arg: ServerArg,
    pub http_arg: ServerArg,
    pub scc: ShareRw<StreamConfigContext>,
    pub request: Option<Request<Body>>,
    pub response_tx: Option<tokio::sync::oneshot::Sender<Response<Body>>>,
}

impl HttpStream {
    pub fn new(
        arg: ServerArg,
        http_arg: ServerArg,
        scc: ShareRw<StreamConfigContext>,
        request: Request<Body>,
        response_tx: tokio::sync::oneshot::Sender<Response<Body>>,
    ) -> HttpStream {
        HttpStream {
            arg,
            http_arg,
            scc,
            request: Some(request),
            response_tx: Some(response_tx),
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
        let ret = stream_start::do_start(self, stream_info, shutdown_thread_rx).await;
        ret
    }
}

#[async_trait]
impl proxy::Stream for HttpStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>) -> Result<()> {
        stream_info
            .get_mut()
            .add_work_time1("net_server_proxy_http");

        let session_id = self.http_arg.stream_info.get().session_id;
        let mut request = self.request.take().unwrap();
        log::debug!(target: "ext3", "r.session_id:{}, client headers = {:#?}", session_id, request);
        let mut part = copy_request_parts(&request);
        let (is_client_sendfile, is_upstream_sendfile, client, uri) =
            self.stream_parse(session_id, &mut request).await?;
        part.uri = uri;

        let mut r = Arc::new(
            HttpStreamRequest::new(
                self.arg.clone(),
                self.http_arg.clone(),
                self.scc.clone(),
                session_id,
                self.response_tx.take().unwrap(),
                part,
                request,
                is_client_sendfile,
                is_upstream_sendfile,
                Some(client).into(),
            )
            .await?,
        );

        r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();

        let plugin_http_header_in = {
            use crate::config::net_core_plugin;
            let ms = self.scc.get().ms.clone();
            let http_core_plugin_main_conf = net_core_plugin::main_conf(&ms).await;
            let plugin_http_header_in = http_core_plugin_main_conf.plugin_http_header_in.clone();
            plugin_http_header_in
        };

        let plugin_http_header_in = plugin_http_header_in.get().await;
        (plugin_http_header_in)(r.clone()).await?;

        self.proxy_cache_parse(&mut r).await?;

        if r.http_cache_file.proxy_cache.is_some() {
            HttpCacheFile::load_cache_file(
                &r,
                r.http_cache_file.proxy_cache.as_ref().unwrap(),
                &r.http_cache_file.cache_file_info,
            )
            .await?;
        }

        let ret = self.do_stream(r.clone()).await.map_err(|e| {
            anyhow!(
                "err:do_stream => request_id:{}, e:{}",
                stream_info.get().request_id,
                e
            )
        });
        if let Err(_) = &ret {
            self.stream_end_err(&r).await?;
        }
        Self::stream_end_free(&r).await?;

        self.cache_file_node_to_pool(r.clone()).await?;
        return ret;
    }
}

impl HttpStream {
    async fn do_stream(&mut self, r: Arc<HttpStreamRequest>) -> Result<()> {
        let (client, is_upstream_sendfile) = {
            let r_ctx = r.ctx.get();
            (r.client.clone().unwrap(), r_ctx.is_upstream_sendfile)
        };

        if !r.ctx.get().is_request_cache {
            return self.stream_not_cache_request(&r, client.clone()).await;
        }

        r.ctx.get_mut().is_try_cache = true;
        if !HttpStream::is_last_upstream_cache(&r).await? {
            r.ctx.get_mut().is_request_cache = false;
            return self.stream_not_cache_request(&r, client.clone()).await;
        }

        if self
            .stream_cache_request_not_get(&r, client.clone())
            .await?
        {
            return Ok(());
        }

        self.get_or_update_cache_file_node(&r, client.clone())
            .await?;

        let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            cache_file_node.update_file_node_expires_time_del();
        }

        loop {
            let ups_request = self.create_slice_request(r.clone(), client.clone()).await?;
            if ups_request.is_none() {
                break;
            }

            self.stream_slice(
                r.clone(),
                client.clone(),
                ups_request.unwrap(),
                is_upstream_sendfile,
            )
            .await?;
        }

        self.http_arg
            .stream_info
            .get_mut()
            .add_work_time1("stream_to_stream end");
        return Ok(());
    }

    pub async fn stream_end_free(r: &Arc<HttpStreamRequest>) -> Result<()> {
        let client_write_tx = r.ctx.get_mut().client_write_tx.take();
        if client_write_tx.is_some() {
            log::trace!(target: "ext", "r.session_id:{}-{} drop client_write_tx", r.session_id, r.local_cache_req_count);
            drop(client_write_tx.unwrap());
        }

        log::trace!(target: "ext", "r.session_id:{}-{} wait executor_client_write", r.session_id, r.local_cache_req_count);
        let executor_client_write = r.ctx.get_mut().executor_client_write.take();
        if executor_client_write.is_some() {
            let executor_client_write = executor_client_write.unwrap();
            executor_client_write.wait("executor_client_write").await?;
        }
        log::trace!(target: "ext", "r.session_id:{}-{} end", r.session_id, r.local_cache_req_count);
        Ok(())
    }

    async fn stream_end_err(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let response_tx = r.ctx.get_mut().response_tx.take();
        if response_tx.is_some() {
            let response_tx = response_tx.unwrap();
            let _ = response_tx.send(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::default())?,
            );
        }

        let (is_upstream, slice_upstream_index) = {
            let r_ctx = r.ctx.get();
            (r_ctx.is_upstream, r_ctx.slice_upstream_index)
        };

        if is_upstream {
            let cache_file_node_manage = r
                .http_cache_file
                .ctx_thread
                .get()
                .cache_file_node_manage
                .clone();
            let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
            cache_file_node_manage.is_upstream = false;
            let mut upstream_waits = VecDeque::with_capacity(10);
            swap(
                &mut upstream_waits,
                &mut cache_file_node_manage.upstream_waits,
            );
            for tx in upstream_waits {
                let _ = tx.send(());
            }
        }

        if slice_upstream_index >= 0 {
            let slice_upstream_index = slice_upstream_index as usize;
            let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
            if cache_file_node.is_some() {
                let cache_file_node = cache_file_node.unwrap();
                let slice_upstream_map =
                    cache_file_node.ctx_thread.get().slice_upstream_map.clone();
                let slice_upstream = slice_upstream_map.get().get(&slice_upstream_index).cloned();
                if slice_upstream.is_some() {
                    let slice_upstream = slice_upstream.unwrap();
                    slice_upstream.get_mut().is_upstream = false;
                    let mut upstream_waits = VecDeque::with_capacity(10);
                    swap(
                        &mut upstream_waits,
                        &mut slice_upstream.get_mut().upstream_waits,
                    );
                    for tx in upstream_waits {
                        let _ = tx.send(());
                    }
                }
            }
        }
        Ok(())
    }

    async fn proxy_cache_parse(&mut self, r: &mut Arc<HttpStreamRequest>) -> Result<()> {
        let (is_local_cache_req, http_cache_file) = {
            let (md5, cache_control_time, is_request_cache) = {
                let stream_info = r.http_arg.stream_info.clone();
                let stream_info = &*stream_info.get();

                let net_curr_conf = self.scc.get().net_curr_conf();
                let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);

                let mut proxy_cache_key_vars = Var::copy(&net_core_proxy_conf.proxy_cache_key_vars)
                    .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
                proxy_cache_key_vars.for_each(|var| {
                    let var_name = Var::var_name(var);
                    let mut is_method = false;
                    if var_name == "http_ups_request_method" || var_name == "http_request_method" {
                        is_method = true;
                    }
                    let value = stream_var::find(var_name, stream_info);
                    match value {
                        Err(e) => {
                            log::error!("{}", anyhow!("{}", e));
                            Ok(None)
                        }
                        Ok(value) => {
                            if value.is_some() {
                                let value = value.unwrap();
                                if is_method && value.to_str().unwrap() == "HEAD" {
                                    Ok(Some(VarAnyData::Str("GET".to_string())))
                                } else {
                                    Ok(Some(value))
                                }
                            } else {
                                Ok(value)
                            }
                        }
                    }
                })?;

                let proxy_cache_key = proxy_cache_key_vars
                    .join()
                    .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
                log::debug!(
                    "r.session_id:{}, proxy_cache_key:{}",
                    r.session_id,
                    proxy_cache_key
                );
                let md5 = proxy_cache_key;

                let r_ctx = &mut r.ctx.get_mut();
                let r_in = &mut r_ctx.r_in;

                let (cache_control_time, _expires_time, _expires_time_sys) =
                    cache_control_time(&r_in.headers_upstream)?;

                let is_request_cache = if cache_control_time == 0 || !r_in.is_version1_upstream {
                    false
                } else {
                    true
                };
                (md5, cache_control_time, is_request_cache)
            };

            let md5 = md5::compute(&md5);
            let md5 = format!("{:x}", md5);
            let crc32 = crc32fast::hash(md5.as_bytes()) as usize;

            log::debug!(
                "r.session_id:{}, is_request_cache:{}",
                r.session_id,
                is_request_cache
            );

            if !is_request_cache && r.local_cache_req_count > 0 {
                return Err(anyhow!("!is_request_cache && local_cache_req_count > 0"));
            }

            let mut is_local_cache_req = false;
            //___wait___
            let is_host = false;

            use crate::config::common_core;
            use crate::config::net_core;
            let (net_curr_conf, common_core_any_conf) = {
                let scc = self.scc.get();
                (scc.net_curr_conf(), scc.common_core_any_conf())
            };
            let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);
            let net_core_conf = net_core::curr_conf(&net_curr_conf);
            let common_coree_conf = common_core::curr_conf(&common_core_any_conf);

            let (proxy_cache, proxy_cache_path, proxy_cache_path_tmp) =
                if is_request_cache && !net_core_proxy_conf.proxy_caches.is_empty() {
                    let index = crc32 % net_core_proxy_conf.proxy_caches.len();
                    let index = if r.local_cache_req_count <= 0
                        && is_host
                        && net_core_proxy_conf.proxy_caches.len() > 1
                    {
                        let other_index: usize = rand::thread_rng().gen();
                        let other_index = other_index % net_core_proxy_conf.proxy_caches.len();
                        if other_index == index {
                            // is_local_cache_req = true;
                            // (index + 1) % net_core_proxy_conf.proxy_caches.len()
                            index
                        } else {
                            is_local_cache_req = true;
                            other_index
                        }
                    } else {
                        index
                    };

                    let proxy_cache = net_core_proxy_conf.proxy_caches[index].clone();
                    let mut proxy_cache_path = proxy_cache.cache_conf.path.clone();

                    let mut levels_len = md5.len();
                    for v in &proxy_cache.levels {
                        let mut v = *v;
                        if v >= md5.len() {
                            v = 2;
                        }
                        proxy_cache_path.push_str(&md5[levels_len - v..levels_len]);
                        proxy_cache_path.push_str("/");
                        levels_len -= v;
                    }
                    let mut proxy_cache_path_tmp = proxy_cache_path.clone();
                    proxy_cache_path_tmp.push_str("tmp/");

                    if !Path::new(&proxy_cache_path).exists() {
                        std::fs::create_dir_all(&proxy_cache_path)
                            .map_err(|e| anyhow!("err:create_dir_all => e:{}", e))?;
                    }
                    if !Path::new(&proxy_cache_path_tmp).exists() {
                        std::fs::create_dir_all(&proxy_cache_path_tmp)
                            .map_err(|e| anyhow!("err:create_dir_all => e:{}", e))?;
                    }

                    let tmpfile_id = common_coree_conf.tmpfile_id.fetch_add(1, Ordering::Relaxed);
                    let pid = unsafe { libc::getpid() };

                    proxy_cache_path_tmp.push_str(&format!("{}_{}_{}", md5, pid, tmpfile_id));
                    proxy_cache_path.push_str(&md5);
                    log::debug!(
                        "r.session_id:{}, proxy_cache_path:{}, proxy_cache_path_tmp:{}",
                        r.session_id,
                        proxy_cache_path,
                        proxy_cache_path_tmp
                    );
                    let proxy_cache_path_tmp = ArcString::from(proxy_cache_path_tmp);
                    let proxy_cache_path = ArcString::from(proxy_cache_path);
                    (Some(proxy_cache), proxy_cache_path, proxy_cache_path_tmp)
                } else {
                    let proxy_cache_path_tmp = ArcString::from("");
                    let proxy_cache_path = ArcString::from("");
                    (None, proxy_cache_path, proxy_cache_path_tmp)
                };

            let md5 = Bytes::from(md5);
            if proxy_cache.is_some() {
                let proxy_cache = proxy_cache.as_ref().unwrap();
                del_expires_cache_file(&md5, proxy_cache).await?;
            }

            let is_request_cache = if proxy_cache.is_none() || !is_request_cache {
                false
            } else {
                true
            };
            log::debug!(
                "r.session_id:{}, is_request_cache:{}",
                r.session_id,
                is_request_cache
            );

            let cache_file_slice = r.cache_file_slice;
            let http_cache_file = HttpCacheFile {
                ctx_thread: ArcRwLock::new(HttpCacheFileContext {
                    cache_file_node_manage: ArcRwLockTokio::default(),
                    cache_file_node: None,
                    cache_file_node_version: 0,
                    cache_file_status: None,
                }),
                proxy_cache,
                cache_file_info: Arc::new(ProxyCacheFileInfo {
                    directio: net_core_conf.directio,
                    cache_file_slice,
                    md5,
                    crc32,
                    proxy_cache_path,
                    proxy_cache_path_tmp,
                }),
            };

            let r_ctx = &mut r.ctx.get_mut();
            let r_in = &mut r_ctx.r_in;
            r_in.cache_control_time = cache_control_time;
            r_ctx.is_request_cache = is_request_cache;
            (is_local_cache_req, http_cache_file)
        };

        r.http_arg.stream_info.get_mut().http_r.set_nil();

        let _r = Arc::get_mut(r).unwrap();
        _r.is_local_cache_req = is_local_cache_req;
        _r.http_cache_file = Some(http_cache_file).into();

        r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();

        Ok(())
    }

    async fn stream_parse(
        &mut self,
        session_id: u64,
        request: &mut Request<Body>,
    ) -> Result<(
        bool,
        bool,
        Arc<hyper::Client<HttpHyperConnector>>,
        http::Uri,
    )> {
        let scc = self.scc.clone();
        let stream_info = self.http_arg.stream_info.clone();

        let (is_proxy_protocol_hello, connect_func) =
            proxy_util::upsteam_connect_info(stream_info.clone(), scc.clone()).await?;

        let hello =
            proxy_util::get_proxy_hello(is_proxy_protocol_hello, stream_info.clone(), scc.clone())
                .await;

        stream_info.get_mut().err_status = ErrStatus::ServiceUnavailable;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            let hello_str = general_purpose::STANDARD.encode(hello_str);

            stream_info.get_mut().upstream_protocol_hello_size = hello_str.len();
            request.headers_mut().insert(
                HeaderName::from_bytes(HTTP_HELLO_KEY.as_bytes())?,
                HeaderValue::from_bytes(hello_str.as_bytes())?,
            );
        }

        let socket_fd = {
            let stream_info = stream_info.get();
            stream_info.server_stream_info.raw_fd
        };

        let version = request.version();
        let protocol7 = connect_func.protocol7().await;
        let upstream_is_tls = connect_func.is_tls().await;

        let upstream_host = connect_func.host().await?;
        let (_, upstream_port) = host_and_port(upstream_host.as_str());
        let req_host = request.headers().get(HOST).cloned();
        if req_host.is_none() {
            return Err(anyhow!("host nil"));
        }
        let req_host = req_host.unwrap();
        let req_host = req_host.to_str().unwrap();
        let (req_host, req_port) = host_and_port(req_host);

        let proxy = {
            let scc = scc.get();
            use crate::config::net_server_proxy_http;
            let http_server_proxy_conf = net_server_proxy_http::currs_conf(scc.net_server_confs());
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
                stream_info.get_mut().upstream_protocol77 = Some(Protocol77::Http2);
            }
            _ => {
                stream_info.get_mut().upstream_protocol77 = Some(Protocol77::Http);
            }
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

        let (is_client_sendfile, is_upstream_sendfile) = StreamStream::is_sendfile(
            client_stream_fd,
            upstream_stream_fd,
            scc.clone(),
            stream_info.clone(),
        );

        let scheme = if self.http_arg.stream_info.get().server_stream_info.is_tls {
            "https"
        } else {
            "http"
        };
        let upstream_scheme = if upstream_is_tls { "https" } else { "http" };
        let upstream_uri = format!(
            "{}://{}:{}{}",
            upstream_scheme,
            req_host,
            upstream_port,
            request
                .uri()
                .path_and_query()
                .map(|x| x.as_str())
                .unwrap_or("/")
        );

        let uri = format!(
            "{}://{}:{}{}",
            scheme,
            req_host,
            req_port,
            request
                .uri()
                .path_and_query()
                .map(|x| x.as_str())
                .unwrap_or("/")
        );

        log::debug!(
            "session_id:{}, upstream_uri = {}, uri = {}",
            session_id,
            upstream_uri,
            uri
        );
        let upstream_uri: http::Uri = upstream_uri.parse()?;
        let uri: http::Uri = uri.parse()?;

        let client = self
            .get_client(
                session_id,
                upstream_version,
                connect_func.clone(),
                &proxy,
                stream_info.clone(),
                &protocol7,
            )
            .await
            .map_err(|e| anyhow!("err:get_client => protocol7:{}, e:{}", protocol7, e))?;

        *request.uri_mut() = upstream_uri;
        request.headers_mut().remove(HOST);
        request.headers_mut().remove("connection");
        *request.version_mut() = upstream_version;

        return Ok((is_client_sendfile, is_upstream_sendfile, client, uri));
    }

    async fn stream_not_cache_request(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<()> {
        let is_slice = false;
        let http_cache_status = HttpCacheStatus::Bypass;
        let request_server = {
            let r_in = &mut r.ctx.get_mut().r_in;
            let mut request_server = Request::new(r_in.body.take().unwrap());

            *request_server.method_mut() = r_in.method_upstream.clone();
            *request_server.uri_mut() = r_in.uri_upstream.clone();
            *request_server.version_mut() = r_in.version_upstream.clone();
            *request_server.headers_mut() = r_in.headers_upstream.clone();
            *request_server.extensions_mut() = r_in.extensions_upstream.take().unwrap();
            request_server
        };

        let ups_request = HttpRequest::Request(request_server);
        self.stream_request(
            r.clone(),
            client.clone(),
            ups_request,
            is_slice,
            http_cache_status,
        )
        .await
    }

    async fn get_cache_file_node(
        &mut self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<(bool, i64, Option<Arc<AtomicI64>>)> {
        let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
        if cache_file_node.is_some() {
            let ctx = cache_file_node.as_ref().unwrap().ctx_thread.get();
            let cache_file_ctx = &mut *r.http_cache_file.ctx_thread.get_mut();
            cache_file_ctx.cache_file_node_version = ctx.cache_file_node_version;
            return Ok((true, -1, None));
        }

        let (
            is_upstream,
            cache_file_status,
            cache_file_node_manage,
            cache_file_node,
            upstream_count,
            upstream_count_drop,
        ) = r.http_cache_file.get_cache_file_node().await?;

        {
            let cache_file_node_manage_ = cache_file_node_manage.get().await;
            let cache_file_ctx = &mut *r.http_cache_file.ctx_thread.get_mut();
            cache_file_ctx.cache_file_node_version =
                cache_file_node_manage_.cache_file_node_version;
            cache_file_ctx.cache_file_node_manage = cache_file_node_manage.clone();
            cache_file_ctx.cache_file_status = cache_file_status;
        }
        r.http_cache_file.ctx_thread.get_mut().cache_file_node = cache_file_node.clone();

        if cache_file_node.is_none() || is_upstream {
            return Ok((false, upstream_count, upstream_count_drop));
        }
        return Ok((true, upstream_count, upstream_count_drop));
    }

    async fn stream_cache_request_not_get(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<bool> {
        {
            let r_ctx = &mut *r.ctx.get_mut();
            if r_ctx.r_in.is_get || r_ctx.r_in.is_head {
                return Ok(false);
            }
            r_ctx.is_upstream = false;
        }

        let is_slice = false;
        let mut _http_cache_status = HttpCacheStatus::Bypass;
        let (is_ok, upstream_count, upstream_count_drop) = self.get_cache_file_node(&r).await?;
        let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
        let ups_request = if is_ok {
            let cache_file_node = cache_file_node.unwrap();
            cache_file_node.update_file_node_expires_time_del();
            let ctx = cache_file_node.ctx_thread.get();
            if let &CacheFileStatus::Expire = &ctx.cache_file_status {
                log::trace!(target: "ext", "r.session_id:{}-{}, Expire file to Exist",
                            r.session_id, r.local_cache_req_count);
            }
            log::trace!(target: "ext", "r.session_id:{}-{}, file_response_not_get",
                            r.session_id, r.local_cache_req_count);

            let request = r.http_cache_file.cache_file_request_not_get();
            if request.is_none() {
                return Err(anyhow!("err:request.is_none()"));
            }
            _http_cache_status = HttpCacheStatus::Hit;
            HttpRequest::CacheFileRequest(request.unwrap())
        } else {
            let request_server = {
                let r_in = &mut r.ctx.get_mut().r_in;
                let mut request_server = Request::new(r_in.body.take().unwrap());

                *request_server.method_mut() = r_in.method_upstream.clone();
                *request_server.uri_mut() = r_in.uri_upstream.clone();
                *request_server.version_mut() = r_in.version_upstream.clone();
                *request_server.headers_mut() = r_in.headers_upstream.clone();
                *request_server.extensions_mut() = r_in.extensions_upstream.take().unwrap();
                request_server
            };

            let mut ups_request = {
                let r_ctx = &mut *r.ctx.get_mut();
                r_ctx.is_upstream = true;
                if upstream_count > r_ctx.max_upstream_count {
                    r_ctx.max_upstream_count = upstream_count;
                }
                r_ctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);
                request_server
            };
            _http_cache_status = self
                .check_upstream_header(&r, cache_file_node, ups_request.headers_mut())
                .await?;
            HttpRequest::Request(ups_request)
        };

        self.stream_request(
            r.clone(),
            client.clone(),
            ups_request,
            is_slice,
            _http_cache_status,
        )
        .await?;
        Ok(true)
    }

    async fn is_last_upstream_cache(r: &Arc<HttpStreamRequest>) -> Result<bool> {
        let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
            r.http_cache_file.proxy_cache.as_ref().unwrap(),
            &r.http_cache_file.cache_file_info.md5,
        );
        let is_last_upstream_cache = cache_file_node_manage.get().await.is_last_upstream_cache;
        Ok(is_last_upstream_cache)
    }

    pub async fn set_is_last_upstream_cache(
        r: &HttpStreamRequest,
        is_last_upstream_cache: bool,
    ) -> Result<()> {
        let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
            r.http_cache_file.proxy_cache.as_ref().unwrap(),
            &r.http_cache_file.cache_file_info.md5,
        );
        cache_file_node_manage
            .get_mut()
            .await
            .is_last_upstream_cache = is_last_upstream_cache;
        Ok(())
    }

    async fn get_or_update_cache_file_node(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<()> {
        let is_slice = true;
        let mut http_cache_status = HttpCacheStatus::Bypass;
        r.ctx.get_mut().is_upstream = false;
        let (is_ok, upstream_count, upstream_count_drop) = self.get_cache_file_node(&r).await?;
        let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
        let ups_request: Result<Option<HttpRequest>> = async {
            if is_ok {
                let cache_file_node = cache_file_node.unwrap();
                {
                    let ctx = cache_file_node.ctx_thread.get();
                    if let &CacheFileStatus::Expire = &ctx.cache_file_status {
                        log::trace!(target: "ext", "r.session_id:{}-{}, Expire file to Exist",
                                    r.session_id, r.local_cache_req_count);
                    }
                }
                let raw_content_length = cache_file_node.response_info.range.raw_content_length;
                let ret = get_http_filter_header_range(&r, raw_content_length).await;
                if ret.is_err() {
                    r.ctx.get_mut().r_in.is_load_range = false;
                    http_cache_status = HttpCacheStatus::Hit;
                    log::trace!(target: "ext", "r.session_id:{}-{}, header_range not, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(r.http_cache_file.cache_file_request_head()?)));
                }

                if r.ctx.get().r_in.is_head {
                    log::trace!(target: "ext", "r.session_id:{}-{}, http head, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(r.http_cache_file.cache_file_request_head()?)));
                }
                log::trace!(target: "ext", "r.session_id:{}-{}, http get wait slice start",
                            r.session_id, r.local_cache_req_count);
                return Ok(None);
            }

            let mut ups_request = {
                log::trace!(target: "ext", "r.session_id:{}-{}, get upstream head http_cache_status:{:?}",
                            r.session_id, r.local_cache_req_count, http_cache_status);
                let r_ctx = &mut *r.ctx.get_mut();
                r_ctx.is_upstream = true;
                if upstream_count > r_ctx.max_upstream_count {
                    r_ctx.max_upstream_count = upstream_count;
                }
                r_ctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);
                let mut ups_request = Request::builder().body(Body::default())?;
                *ups_request.method_mut() = http::Method::HEAD;
                *ups_request.uri_mut() = r_ctx.r_in.uri_upstream.clone();
                *ups_request.version_mut() = r_ctx.r_in.version_upstream.clone();
                *ups_request.headers_mut() = r_ctx.r_in.headers_upstream.clone();
                ups_request.headers_mut().insert(
                    http::header::RANGE,
                    HeaderValue::from_bytes("bytes=0-".as_bytes())?,
                );
                ups_request
            };

            http_cache_status = self.check_upstream_header(&r, cache_file_node, ups_request
                .headers_mut()).await?;
                return Ok(Some(HttpRequest::Request(ups_request)));
        } .await;

        let ups_request = ups_request?;
        if ups_request.is_none() {
            return Ok(());
        }
        let ups_request = ups_request.unwrap();
        self.stream_request(
            r.clone(),
            client.clone(),
            ups_request,
            is_slice,
            http_cache_status,
        )
        .await
    }

    async fn check_upstream_header(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        cache_file_node: Option<Arc<ProxyCacheFileNode>>,
        headers: &mut http::HeaderMap<HeaderValue>,
    ) -> Result<HttpCacheStatus> {
        if cache_file_node.is_some() {
            log::trace!(target: "ext",
                        "r.session_id:{}-{}, Expire file is_upstream",
                        r.session_id,
                        r.local_cache_req_count
            );

            let cache_file_node = cache_file_node.unwrap();
            let response_info = &cache_file_node.response_info;
            headers.insert(http::header::ETAG, response_info.e_tag.clone());
            headers.insert(
                http::header::LAST_MODIFIED,
                response_info.last_modified.clone(),
            );
            return Ok(HttpCacheStatus::Expired);
        }
        log::trace!(target: "ext",
                    "r.session_id:{}-{}, create file",
                    r.session_id,
                    r.local_cache_req_count
        );

        headers.remove(http::header::ETAG);
        headers.remove(http::header::LAST_MODIFIED);

        headers.remove(http::header::IF_NONE_MATCH);
        headers.remove(http::header::IF_MATCH);

        return Ok(HttpCacheStatus::Create);
    }

    /*
    async fn stream_create(
        &mut self,
        r: Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<()> {
        let mut is_slice = true;
        let mut _cache_status = HttpCacheStatus::Bypass;
        let ups_request: Result<Option<HttpRequest>> = async {
            let (left_content_length, is_request_cache) = {
                let r_ctx = r.ctx.get();
                (r_ctx.r_in.left_content_length, r_ctx.is_request_cache)
            };
            if left_content_length < 0 && !is_request_cache {
                _cache_status = HttpCacheStatus::Bypass;
                is_slice = false;
                return Ok(Some(HttpRequest::Request(
                    r.ctx.get_mut().r_in.request_server.take().unwrap(),
                )));
            }

            loop {
                r.ctx.get_mut().is_upstream = false;
                let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
                let cache_file_node = if cache_file_node.is_none() {
                    let (is_upstream, cache_file_status, cache_file_node_manage, cache_file_node) =
                        r.http_cache_file.get_cache_file_node().await?;

                    {
                        let cache_file_node_manage_ = cache_file_node_manage.get().await;
                        let req_cache_file_ctx = &mut *r.http_cache_file.ctx_thread.get_mut();
                        req_cache_file_ctx.cache_file_node_version = cache_file_node_manage_.cache_file_node_version;
                        req_cache_file_ctx.cache_file_node_manage = cache_file_node_manage.clone();
                        req_cache_file_ctx.cache_file_status = cache_file_status;
                    }
                    if cache_file_node.is_none() {
                        log::trace!(target: "ext",
                                    "r.session_id:{}-{}, create file",
                            r.session_id,
                            r.local_cache_req_count
                        );
                        break;
                    }
                    r.http_cache_file.ctx_thread.get_mut().cache_file_node = cache_file_node.clone();
                    if is_upstream {
                        log::trace!(target: "ext",
                                    "r.session_id:{}-{}, Expire file is_upstream",
                                    r.session_id,
                                    r.local_cache_req_count
                        );
                        break;
                    }
                    cache_file_node.unwrap()
                } else {
                    cache_file_node.unwrap()
                };

                {
                    let ctx = cache_file_node.ctx_thread.get();
                    {
                        let req_cache_file_ctx = &mut *r.http_cache_file.ctx_thread.get_mut();
                        req_cache_file_ctx.cache_file_node_version = ctx.cache_file_node_version;
                    }
                    if let &CacheFileStatus::Expire = &ctx.cache_file_status {
                        if !ctx.bitmap.get().is_full() {
                            log::trace!(target: "ext", "r.session_id:{}-{}, Expire file2 is_upstream",
                                        r.session_id, r.local_cache_req_count);
                            break;
                        }

                        log::trace!(target: "ext", "r.session_id:{}-{}, Expire file to Exist",
                                    r.session_id, r.local_cache_req_count);
                    }
                }

                {
                    let r_ctx = &mut *r.ctx.get_mut();
                    if left_content_length < 0 && !r_ctx.r_in.is_get && !r_ctx.r_in.is_head {
                        log::trace!(target: "ext", "r.session_id:{}-{}, file_response_not_get",
                                    r.session_id, r.local_cache_req_count);

                        is_slice = false;
                        let request = r.http_cache_file.cache_file_request_not_get();
                        if request.is_none() {
                            r.http_cache_file.ctx_thread.get_mut().cache_file_node = None;
                            break;
                        }
                        _cache_status = HttpCacheStatus::Hit;
                        return Ok(Some(HttpRequest::CacheFileRequest(request.unwrap())));
                    }
                }

                let raw_content_length = cache_file_node.response_info.range.raw_content_length;
                let ret = get_http_filter_header_range(&r, raw_content_length).await;
                if ret.is_err() {
                    r.ctx.get_mut().r_in.is_load_range = false;
                    _cache_status = HttpCacheStatus::Hit;
                    log::trace!(target: "ext", "r.session_id:{}-{}, header_range not, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(r.http_cache_file.cache_file_request_head()?)));
                }

                if r.ctx.get().r_in.is_head {
                    log::trace!(target: "ext", "r.session_id:{}-{}, http head, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(r.http_cache_file.cache_file_request_head()?)));
                }
                log::trace!(target: "ext", "r.session_id:{}-{}, http get wait slice start",
                            r.session_id, r.local_cache_req_count);
                return Ok(None);
            }

            let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
            if cache_file_node.is_some() {
                log::trace!(target: "ext",
                            "r.session_id:{}-{}, Expire file is_upstream",
                            r.session_id,
                            r.local_cache_req_count
                );
                _cache_status = HttpCacheStatus::Expired;
            } else {
                log::trace!(target: "ext",
                            "r.session_id:{}-{}, create file",
                            r.session_id,
                            r.local_cache_req_count
                );
                _cache_status = HttpCacheStatus::Create;
            }

            let r_ctx = &mut *r.ctx.get_mut();
            r_ctx.is_upstream = true;
            let mut ups_request =
                if left_content_length < 0 && !r_ctx.r_in.is_get && !r_ctx.r_in.is_head {
                    log::trace!(target: "ext", "r.session_id:{}-{}, not(get and head) upstream head",
                                r.session_id, r.local_cache_req_count);
                    is_slice = false;
                    r_ctx.r_in.request_server.take().unwrap()
                } else {
                    log::trace!(target: "ext", "r.session_id:{}-{}, get upstream head _cache_status:{:?}",
                                r.session_id, r.local_cache_req_count, _cache_status);
                    let mut ups_request = Request::builder().body(Body::default())?;
                    *ups_request.method_mut() = http::Method::HEAD;
                    *ups_request.uri_mut() = r_ctx.r_in.uri_upstream.clone();
                    *ups_request.version_mut() = r_ctx.r_in.version_upstream.clone();
                    *ups_request.headers_mut() = r_ctx.r_in.headers_upstream.clone();
                    ups_request.headers_mut().insert(
                        http::header::RANGE,
                        HeaderValue::from_bytes("bytes=0-".as_bytes())?,
                    );
                    ups_request
                };
            if cache_file_node.is_some() {
                let cache_file_node = cache_file_node.unwrap();
                let response_info = &cache_file_node.response_info;
                ups_request
                    .headers_mut()
                    .insert(http::header::ETAG, response_info.e_tag.clone());
                ups_request
                    .headers_mut()
                    .insert(http::header::LAST_MODIFIED, response_info.last_modified.clone());
            } else {
                ups_request.headers_mut().remove(http::header::ETAG);
                ups_request
                    .headers_mut()
                    .remove(http::header::LAST_MODIFIED);

                ups_request
                    .headers_mut()
                    .remove(http::header::IF_NONE_MATCH);
                ups_request.headers_mut().remove(http::header::IF_MATCH);
            }
            return Ok(Some(HttpRequest::Request(ups_request)));
        }
        .await;
        let ups_request = ups_request?;
        if ups_request.is_none() {
            return Ok(());
        }

        let ups_request = ups_request.unwrap();

        let is_upstream_sendfile = async {
            let r_ctx = &mut *r.ctx.get_mut();
            r_ctx.r_in.is_slice = is_slice;
            r_ctx.r_in.http_cache_status = _cache_status;
            r_ctx.is_upstream_sendfile
        }
        .await;

        self.do_stream_request(r.clone(), client.clone(), ups_request, is_upstream_sendfile)
            .await
    }
     */

    pub async fn create_slice_request(
        &mut self,
        r: Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<Option<HttpRequest>> {
        let (range_start, range_end, slice_end) = {
            let ctx = &mut *r.ctx.get_mut();
            ctx.slice_upstream_index = -1;
            log::trace!(target: "ext", "r.session_id:{}-{}, left_content_length:{}",
                        r.session_id, r.local_cache_req_count, ctx.r_in.left_content_length);
            if ctx.r_in.left_content_length <= 0 {
                return Ok(None);
            }

            let range_start = ctx.r_in.curr_slice_start;
            let range_end = ctx.r_in.curr_slice_start + r.http_request_slice - 1;
            let slice_end = ctx.r_in.slice_end;
            if range_start % r.http_request_slice != 0 || range_end > ctx.r_in.slice_end {
                return Err(anyhow::anyhow!(
                    "err:curr_slice_start => curr_slice_start:{}, ctx.r_in.slice_end:{}, left_content_length:{}",
                    ctx.r_in.curr_slice_start, ctx.r_in.slice_end, ctx.r_in.left_content_length
                ));
            }
            (range_start, range_end, slice_end)
        };

        let slice_index = align_bitset_start_index(range_start, r.cache_file_slice)?;
        r.ctx.get_mut().r_in.cur_slice_index = slice_index;
        r.ctx.get_mut().r_in.is_slice = true;
        let is_last_upstream_cache = HttpStream::is_last_upstream_cache(&r).await?;
        let mut version = -1;
        let once_time = 1000 * 10;
        let (upstream_count, upstream_count_drop) = loop {
            if !is_last_upstream_cache {
                break (0, None);
            }

            let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
            let cache_file_node = if cache_file_node.is_none() {
                self.get_or_update_cache_file_node(&r, client.clone())
                    .await?;
                let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
                if cache_file_node.is_none() {
                    break (0, None);
                }
                cache_file_node.unwrap()
            } else {
                cache_file_node.unwrap()
            };

            let slice_upstream_map = cache_file_node.ctx_thread.get().slice_upstream_map.clone();
            let request =
                r.http_cache_file
                    .cache_file_request_get(range_start, range_end, slice_end);
            if request.is_some() {
                log::trace!(target: "ext", "r.session_id:{}-{}, slice cache_file_request_get, slice_index:{} range_start:{}, range_end:{}, slice_end:{}",
                            r.session_id, r.local_cache_req_count, slice_index, range_start, range_end, slice_end);

                let slice_upstream = slice_upstream_map.get().get(&slice_index).cloned();
                if slice_upstream.is_some() {
                    let slice_upstream = slice_upstream.unwrap();
                    let slice_upstream = &mut *slice_upstream.get_mut();
                    let mut upstream_waits = VecDeque::with_capacity(10);
                    swap(&mut upstream_waits, &mut slice_upstream.upstream_waits);
                    for upstream_waits in upstream_waits {
                        let _ = upstream_waits.send(());
                    }
                }
                r.ctx.get_mut().r_in.http_cache_status = HttpCacheStatus::Hit;
                return Ok(Some(HttpRequest::CacheFileRequest(request.unwrap())));
            }

            if slice_upstream_map.get().get(&slice_index).is_none() {
                let cache_file_node_version =
                    FILE_CACHE_NODE_MANAGE_ID.fetch_add(1, Ordering::Relaxed) as u64;
                let cache_file_node_version = (cache_file_node_version << 32) | 0;
                let upstream_count = Arc::new(AtomicI64::new(1));
                slice_upstream_map.get_mut().insert(
                    slice_index,
                    ArcMutex::new(ProxyCacheFileNodeUpstream {
                        is_upstream: true,
                        version: cache_file_node_version as i64,
                        upstream_count: upstream_count.clone(),
                        upstream_waits: VecDeque::with_capacity(10),
                    }),
                );
                log::debug!(
                    "111111 r.session_id:{}, slice_index:{}, slice upstream:{}",
                    r.session_id,
                    slice_index,
                    r.local_cache_req_count
                );
                break (1, Some(upstream_count));
            }

            let rx = {
                let slice_upstream = slice_upstream_map.get().get(&slice_index).cloned().unwrap();
                let slice_upstream = &mut *slice_upstream.get_mut();
                if !slice_upstream.is_upstream
                    || (version >= 0 && version == slice_upstream.version)
                {
                    slice_upstream.is_upstream = true;
                    slice_upstream.version += 1;
                    let upstream_count = slice_upstream
                        .upstream_count
                        .fetch_add(1, Ordering::Relaxed)
                        + 1;
                    log::debug!(
                        "2222222 r.session_id:{}, slice_index:{}, slice upstream:{}",
                        r.session_id,
                        slice_index,
                        r.local_cache_req_count
                    );
                    break (upstream_count, Some(slice_upstream.upstream_count.clone()));
                }
                version = slice_upstream.version;
                let (tx, rx) = tokio::sync::oneshot::channel();
                slice_upstream.upstream_waits.push_back(tx);
                rx
            };

            self.cache_file_node_to_pool(r.clone()).await?;

            tokio::select! {
                biased;
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(once_time)) => {
                }
                _ =  rx => {
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        };

        self.cache_file_node_to_pool(r.clone()).await?;

        log::trace!(target: "ext", "r.session_id:{}-{}, slice upstream, slice_index:{} range_start:{}, range_end:{}, slice_end:{}",
                    r.session_id, r.local_cache_req_count, slice_index, range_start, range_end, slice_end);

        let range_end = if !is_last_upstream_cache {
            r.ctx.get_mut().is_request_cache = false;
            slice_end
        } else {
            self.get_or_update_cache_file_node(&r, client.clone())
                .await?;
            range_end
        };

        let ctx = &mut *r.ctx.get_mut();
        ctx.slice_upstream_index = slice_index as i32;
        ctx.last_slice_upstream_index = slice_index as i32;
        if upstream_count > ctx.max_upstream_count {
            ctx.max_upstream_count = upstream_count;
        }
        ctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);

        ctx.r_in.http_cache_status = HttpCacheStatus::Miss;
        let mut request = Request::builder().body(Body::default())?;
        *request.method_mut() = http::Method::GET;
        *request.uri_mut() = ctx.r_in.uri_upstream.clone();
        *request.version_mut() = ctx.r_in.version_upstream;
        *request.headers_mut() = ctx.r_in.headers_upstream.clone();
        request.headers_mut().insert(
            http::header::RANGE,
            HeaderValue::from_bytes(format!("bytes={}-{}", range_start, range_end).as_bytes())?,
        );
        request.headers_mut().remove(http::header::ETAG);
        request.headers_mut().remove(http::header::LAST_MODIFIED);
        request.headers_mut().remove(http::header::IF_NONE_MATCH);
        request.headers_mut().remove(http::header::IF_MATCH);

        return Ok(Some(HttpRequest::Request(request)));
    }

    async fn stream_slice(
        &mut self,
        r: Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
        ups_request: HttpRequest,
        is_upstream_sendfile: bool,
    ) -> Result<()> {
        let (client_stream_flow_info, upstream_connect_flow_info) = {
            let stream_info = self.http_arg.stream_info.get();
            (
                stream_info.client_stream_flow_info.clone(),
                stream_info.upstream_connect_flow_info.clone(),
            )
        };
        client_stream_flow_info.get_mut().reset_err();
        upstream_connect_flow_info.get_mut().reset_err();

        self.do_stream_request(r.clone(), client.clone(), ups_request, is_upstream_sendfile)
            .await?;

        let cache_file_node = r.http_cache_file.ctx_thread.get().cache_file_node.clone();
        if cache_file_node.is_none() {
            return Ok(());
        }
        let cache_file_node = cache_file_node.unwrap();
        let range_start = {
            let response_info = &r.ctx.get().r_out.response_info;
            let range_start = response_info.as_ref().unwrap().range.range_start;
            range_start
        };
        let slice_index = align_bitset_start_index(
            range_start,
            cache_file_node.cache_file_info.cache_file_slice,
        )?;
        let slice_upstream_map = cache_file_node.ctx_thread.get().slice_upstream_map.clone();
        let cache_file_node_ups = slice_upstream_map.get().get(&slice_index).cloned();
        if cache_file_node_ups.is_some() {
            let cache_file_node_ups = cache_file_node_ups.unwrap();
            let cache_file_node_ups = &mut *cache_file_node_ups.get_mut();
            let mut upstream_waits = VecDeque::with_capacity(10);
            swap(&mut upstream_waits, &mut cache_file_node_ups.upstream_waits);
            for tx in upstream_waits {
                let _ = tx.send(());
            }
        }

        let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
        let is_full = bitmap.get().is_full();
        if is_full {
            let slice_upstream_map = cache_file_node.ctx_thread.get().slice_upstream_map.clone();
            slice_upstream_map.get_mut().clear();
        }

        Ok(())
    }

    async fn cache_file_node_to_pool(&mut self, r: Arc<HttpStreamRequest>) -> Result<()> {
        let cache_file_node = r
            .http_cache_file
            .ctx_thread
            .get_mut()
            .cache_file_node
            .take();
        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            r.http_cache_file
                .cache_file_node_to_pool(cache_file_node)
                .await?;
        }

        return Ok(());
    }

    pub async fn start_upstream_request(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
        ups_request: HttpRequest,
        is_upstream_sendfile: bool,
    ) -> Result<HttpResponse> {
        let ups_response = match ups_request {
            HttpRequest::Request(mut ups_request) => {
                if r.is_local_cache_req {
                    ups_request.headers_mut().insert(
                        LOCAL_CACHE_REQ_KEY,
                        HeaderValue::from(r.local_cache_req_count + 1),
                    );
                }

                r.ctx.get_mut().r_in.head_upstream_size +=
                    http_headers_size(Some(ups_request.uri()), None, ups_request.headers());

                let (_req_parts, client_read) = ups_request.into_parts();
                let (upstream_write, _client_req_body) = Body::channel();
                let mut ups_request = Request::from_parts(_req_parts, _client_req_body);

                let upstream_connect_flow_info = self
                    .http_arg
                    .stream_info
                    .get()
                    .upstream_connect_flow_info
                    .clone();

                let ups_response = if r.is_local_cache_req {
                    *ups_request.uri_mut() = r.ctx.get().r_in.uri.clone();
                    log::debug!(target: "ext3", "r.session_id:{}-{}, slice local ups_request:{:#?}", r.session_id, r.local_cache_req_count,ups_request);
                    let ups_response = http_handle_local(
                        r.clone(),
                        self.arg.clone(),
                        ups_request,
                        |arg, http_arg, scc, request| {
                            Box::pin(http_server_run_handle(arg, http_arg, scc, request))
                        },
                    )
                    .await
                    .map_err(|e| anyhow!("err:http_handle_local =>e:{}", e))?;
                    ups_response
                } else {
                    log::debug!(target: "ext3", "r.session_id:{}-{}, slice upstream ups_request:{:#?}", r.session_id, r.local_cache_req_count,ups_request);
                    let ups_response = client
                        .request(
                            ups_request,
                            Some(ReqArg {
                                upstream_connect_flow_info: upstream_connect_flow_info.clone(),
                            }),
                        )
                        .await
                        .map_err(|e| anyhow!("err:client.request =>e:{}", e))?;
                    ups_response
                };

                let head = ups_response.extensions().get::<hyper::AnyProxyRawHeaders>();
                let head = if head.is_some() {
                    let head = head.unwrap();
                    head.0 .0.clone()
                } else {
                    http_respons_to_vec(&ups_response).into()
                };

                r.ctx.get_mut().r_out.head_upstream_size += head.len();

                if r.ctx.get().r_out_main.is_none() {
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!(
                            "r.session_id:{}, raw head:{}",
                            r.session_id,
                            String::from_utf8_lossy(head.as_ref())
                        );
                    }

                    r.ctx.get_mut().r_out.head = Some(head);
                }

                if !r.ctx.get().r_in.is_body_nil {
                    let header_ext = if r.ctx.get().r_in.is_version1_upstream {
                        let header_ext = ups_response
                            .extensions()
                            .get::<hyper::AnyProxyRawHttpHeaderExt>();
                        if header_ext.is_some() {
                            log::debug!("Response header_ext:{}", r.local_cache_req_count);
                            let header_ext = header_ext.unwrap();
                            header_ext.0 .0.clone()
                        } else {
                            log::debug!("nil Response header_ext:{}", r.local_cache_req_count);
                            HttpHeaderExt::new()
                        }
                    } else {
                        HttpHeaderExt::new()
                    };
                    r.ctx.get_mut().r_out.header_ext = header_ext;

                    self.stream_to_upstream(is_upstream_sendfile, client_read, upstream_write)
                        .await?;
                }

                let (parts, body) = ups_response.into_parts();
                HttpResponse {
                    response: Response::from_parts(parts, Body::empty()),
                    body: HttpResponseBody::Body(body),
                }
            }
            HttpRequest::CacheFileRequest(ups_request) => {
                log::debug!("r.session_id:{}, slice local_req", r.session_id);
                HttpResponse {
                    response: ups_request.response,
                    body: HttpResponseBody::File(ups_request.buf_file),
                }
            }
        };
        Ok(ups_response)
    }

    async fn stream_request(
        &mut self,
        r: Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
        ups_request: HttpRequest,
        is_slice: bool,
        http_cache_status: HttpCacheStatus,
    ) -> Result<()> {
        let is_upstream_sendfile = async {
            let r_ctx = &mut *r.ctx.get_mut();
            r_ctx.r_in.is_slice = is_slice;
            r_ctx.r_in.http_cache_status = http_cache_status;
            r_ctx.is_upstream_sendfile
        }
        .await;
        self.do_stream_request(r, client, ups_request, is_upstream_sendfile)
            .await
    }

    async fn do_stream_request(
        &mut self,
        r: Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
        ups_request: HttpRequest,
        is_upstream_sendfile: bool,
    ) -> Result<()> {
        r.ctx.get_mut().r_in.curr_request_count += 1;
        let (plugin_http_header_filter, plugin_http_body_filter) = {
            use crate::config::net_core_plugin;
            let ms = self.scc.get().ms.clone();
            let http_core_plugin_main_conf = net_core_plugin::main_conf(&ms).await;
            let plugin_http_header_filter =
                http_core_plugin_main_conf.plugin_http_header_filter.clone();
            let plugin_http_body_filter =
                http_core_plugin_main_conf.plugin_http_body_filter.clone();
            (plugin_http_header_filter, plugin_http_body_filter)
        };

        let ups_response = self
            .start_upstream_request(&r, client, ups_request, is_upstream_sendfile)
            .await?;
        log::debug!(target: "ext3",
            "r.session_id:{}-{}, slice ups_response:{:#?}",
            r.session_id,
            r.local_cache_req_count,
            ups_response.response
        );
        let HttpResponse {
            response: ups_response,
            body: upstream_body,
        } = ups_response;
        self.response_to_out(r.clone(), ups_response).await?;

        let plugin_http_header_filter = plugin_http_header_filter.get().await;
        (plugin_http_header_filter)(r.clone()).await?;

        if let HttpResponseBody::Body(_) = &upstream_body {
            update_or_create_cache_file(&r).await?;
        }

        let client_write_tx = r.ctx.get_mut().client_write_tx.take();
        if client_write_tx.is_none() {
            log::trace!(target: "ext", "r.session_id:{}-{}, Response disable body", r.session_id, r.local_cache_req_count);
            return Ok(());
        }
        let mut client_write_tx = client_write_tx.unwrap();
        HttpStream::stream_to_client(
            r.clone(),
            upstream_body,
            plugin_http_body_filter,
            &mut client_write_tx,
        )
        .await?;
        r.ctx.get_mut().client_write_tx = Some(client_write_tx);
        return Ok(());
    }

    async fn response_to_out(
        &mut self,
        r: Arc<HttpStreamRequest>,
        response: Response<Body>,
    ) -> Result<()> {
        let r_ctx = &mut *r.ctx.get_mut();

        r_ctx.r_out.status_upstream = response.status().clone();
        r_ctx.r_out.version_upstream = response.version().clone();
        r_ctx.r_out.headers_upstream = response.headers().clone();

        let (part, _) = response.into_parts();
        let http::response::Parts {
            status,
            version: _,
            headers,
            extensions,
            ..
        } = part;
        r_ctx.r_out.status = status;
        r_ctx.r_out.version = r_ctx.r_in.version;
        r_ctx.r_out.headers = headers;
        r_ctx.r_out.extensions.set(extensions);
        return Ok(());
    }

    async fn stream_to_upstream(
        &mut self,
        is_upstream_sendfile: bool,
        client_read: hyper::body::Body,
        upstream_write: hyper::body::Sender,
    ) -> Result<()> {
        let stream_info = self.http_arg.stream_info.clone();
        let upstream_stream = {
            let stream_info = stream_info.get();
            let stream = Stream::new(client_read, upstream_write);
            let (stream_rx, stream_tx) = any_base::io::split::split(stream);

            let mut read_stream =
                StreamFlow::new2(stream_rx, stream_nil_write::Stream::new(), None);
            read_stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));
            let mut write_stream =
                StreamFlow::new2(stream_nil_read::Stream::new(), stream_tx, None);
            write_stream.set_stream_info(Some(stream_info.upstream_stream_flow_info.clone()));

            let upstream_stream = StreamFlow::new2(read_stream, write_stream, None);
            upstream_stream
        };

        let ret = StreamStream::stream_single(
            self.scc.clone(),
            stream_info.clone(),
            upstream_stream,
            is_upstream_sendfile,
            false,
        )
        .await
        .map_err(|e| anyhow!("err:stream_single =>  e:{}", e));
        if let Err(e) = ret {
            if !stream_info.get().upstream_stream_flow_info.get().is_close() {
                return Err(e);
            }
        }
        return Ok(());
    }

    pub async fn stream_to_client(
        r: Arc<HttpStreamRequest>,
        mut response_body: HttpResponseBody,
        plugin_http_body_filter: ArcRwLockTokio<PluginHttpFilter>,
        client_write_tx: &mut any_base::stream_channel_write::Stream,
    ) -> Result<()> {
        let mut file_cache_bytes = FileCacheBytes::new(r.page_size);
        loop {
            let body_buf = response_body_read(&r, &mut response_body).await?;
            let ret: Result<()> = async {
                let cache_file_node = {
                    let r_ctx = &mut *r.ctx.get_mut();
                    if !r_ctx.r_out.is_cache || r_ctx.r_out.is_cache_err {
                        return Ok(());
                    }
                    r.http_cache_file
                        .ctx_thread
                        .get()
                        .cache_file_node
                        .clone()
                        .unwrap()
                };

                let is_end = if body_buf.is_some() {
                    if let HttpBodyBuf::Bytes(buf) = &body_buf.as_ref().unwrap().buf {
                        file_cache_bytes.push_back(buf.data.clone());
                    }
                    false
                } else {
                    true
                };

                loop {
                    let file_cache_bytes_ = file_cache_bytes.page_chunks_copy(is_end);
                    if file_cache_bytes_.is_none() {
                        break;
                    }

                    let file_seek = r.ctx.get().r_in.bitmap_curr_slice_start
                        + cache_file_node.fix.body_start as u64;
                    let file = cache_file_node.get_file_ext().file.clone();
                    let size =
                        write_cache_file(&r, file, file_seek, file_cache_bytes_.unwrap()).await?;
                    r.ctx.get_mut().r_in.bitmap_curr_slice_start += size as u64;
                    file_cache_bytes.advance(size);
                }

                let is_ok = slice_update_bitset(&r, &cache_file_node).await?;
                if !is_ok {
                    return Ok(());
                }
                bitmap_to_cache_file(&r, &cache_file_node).await?;
                return Ok(());
            }
            .await;
            ret?;

            if body_buf.is_none() {
                break;
            }

            if let HttpBodyBuf::Bytes(buf) = &body_buf.as_ref().unwrap().buf {
                let upstream_stream_flow_info = r
                    .http_arg
                    .stream_info
                    .get()
                    .upstream_stream_flow_info
                    .clone();
                use bytes::Buf;
                upstream_stream_flow_info.get_mut().read += buf.remaining() as i64;
            }

            r.ctx.get_mut().in_body_buf = body_buf;

            let plugin_http_body_filter = plugin_http_body_filter.get().await;
            (plugin_http_body_filter)(r.clone()).await?;

            let body_buf = r.ctx.get_mut().out_body_buf.take();
            write_body_to_client(&r, body_buf, client_write_tx).await?;
        }
        return Ok(());
    }
}
