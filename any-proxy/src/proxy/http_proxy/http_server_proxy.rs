use super::http_hyper_connector::HttpHyperConnector;
use crate::config::config_toml::HttpVersion;
use crate::proxy::http_proxy::stream::Stream;
use crate::proxy::http_proxy::{http_handle_local, HttpHeaderResponse, HTTP_HELLO_KEY};
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::util as proxy_util;
use crate::proxy::ServerArg;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ArcRwLock, ArcRwLockTokio};
use any_base::typ2;
use anyhow::anyhow;
use anyhow::Result;
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
    cache_control_time, content_length, http_headers_size, http_respons_to_vec, is_request_body_nil,
};
use crate::proxy::http_proxy::http_stream_request::{
    CacheFileStatus, HttpBodyBuf, HttpCacheStatus, HttpRequest, HttpResponse, HttpResponseBody,
    HttpStreamRequest, UpstreamCountDrop, LOCAL_CACHE_REQ_KEY,
};
use crate::proxy::http_proxy::util::{
    bitmap_to_cache_file, del_expires_cache_file, del_max_open_cache_file, del_md5,
    response_body_read, slice_update_bitset, update_or_create_cache_file,
    update_or_create_cache_file_304, write_body_to_client, write_cache_file,
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
use http::header::{CONNECTION, HOST};
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

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
    let scc = r.http_arg.stream_info.get().scc.clone();
    use crate::config::net_server_http_proxy;
    let net_server_http_proxy_conf = net_server_http_proxy::curr_conf_mut(scc.net_curr_conf());
    if net_server_http_proxy_conf.proxy.is_none() {
        return Ok(crate::Error::Ok);
    }

    HttpStream::new(&r).do_stream(&r).await?;

    return Ok(crate::Error::Finish);
}

pub struct HttpStream {
    pub arg: ServerArg,
    pub http_arg: ServerArg,
    pub header_response: Arc<HttpHeaderResponse>,
}

impl HttpStream {
    pub fn new(r: &Arc<HttpStreamRequest>) -> HttpStream {
        HttpStream {
            arg: r.arg.clone(),
            http_arg: r.http_arg.clone(),
            header_response: r.ctx.get().header_response.clone().unwrap(),
        }
    }

    async fn do_stream(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let stream_info = r.http_arg.stream_info.clone();
        stream_info
            .get_mut()
            .add_work_time1("net_server_http_proxy");

        let scc = stream_info.get().scc.clone();
        let ms = scc.ms();

        self.upstream_parse(&r).await?;

        if !r.ctx.get().is_request_cache {
            return self.stream_not_cache_request(&r).await;
        }

        self.proxy_cache_parse(&r).await?;

        if !r.ctx.get().is_request_cache {
            return self.stream_not_cache_request(&r).await;
        }

        let http_cache_file = r.ctx.get().http_cache_file.clone();
        if http_cache_file.proxy_cache.is_some() {
            HttpCacheFile::load_cache_file(
                &r,
                http_cache_file.proxy_cache.as_ref().unwrap(),
                &http_cache_file.cache_file_info,
                ms,
                &scc,
            )
            .await?;
        }

        r.ctx.get_mut().is_try_cache = true;
        if !HttpStream::is_last_upstream_cache(&r).await? {
            r.ctx.get_mut().is_request_cache = false;
            return self.stream_not_cache_request(&r).await;
        }

        if self.stream_cache_request_not_range(&r).await? {
            if r.ctx.get().r_in.is_304 {
                if self.stream_cache_request_not_range(&r).await? {
                    return Ok(());
                }
            }
            return Ok(());
        }

        self.get_or_update_cache_file_node(&r).await?;

        loop {
            let ups_request = self.create_slice_request(&r).await?;
            if ups_request.is_none() {
                break;
            }

            self.stream_slice(&r, ups_request.unwrap()).await?;
        }

        self.http_arg
            .stream_info
            .get_mut()
            .add_work_time1("stream_to_stream end");
        return Ok(());
    }

    pub async fn stream_end_free(r: &Arc<HttpStreamRequest>) -> Result<()> {
        let body = {
            let rctx = &mut *r.ctx.get_mut();
            if rctx.r_in.content_length > 0 {
                rctx.r_in.body.take()
            } else {
                None
            }
        };

        if body.is_some() {
            let mut body = body.unwrap();
            use futures_util::StreamExt;
            loop {
                let data = body.next().await;
                if data.is_none() {
                    break;
                }
                let data = data.unwrap();
                if data.is_err() {
                    break;
                }
            }
        }

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

    pub async fn stream_end_err(r: &Arc<HttpStreamRequest>) -> Result<()> {
        let (is_upstream, slice_upstream_index, http_cache_file) = {
            let rctx = r.ctx.get();
            (
                rctx.is_upstream,
                rctx.slice_upstream_index,
                rctx.http_cache_file.clone(),
            )
        };

        if is_upstream {
            let cache_file_node_manage = http_cache_file
                .ctx_thread
                .get()
                .cache_file_node_manage
                .clone();
            let cache_file_node_manage =
                &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
            if r.ctx.get().is_upstream_add {
                log::trace!(target: "is_ups", "session_id:{}, set is_upstream = false", r.session_id);
                r.ctx.get_mut().is_upstream_add = false;
                cache_file_node_manage.is_upstream = false;
                cache_file_node_manage
                    .upstream_count
                    .fetch_sub(1, Ordering::Relaxed);
            }
        }

        if http_cache_file.is_some() {
            let cache_file_node_manage = http_cache_file
                .ctx_thread
                .get()
                .cache_file_node_manage
                .clone();
            if cache_file_node_manage.is_some().await {
                let upstream_waits = {
                    let cache_file_node_manage =
                        &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
                    let mut upstream_waits = VecDeque::with_capacity(10);
                    swap(
                        &mut upstream_waits,
                        &mut cache_file_node_manage.upstream_waits,
                    );
                    upstream_waits
                };
                for tx in upstream_waits {
                    let _ = tx.send(());
                }
            }
        }

        if slice_upstream_index >= 0 {
            let slice_upstream_index = slice_upstream_index as usize;
            let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
            if cache_file_node.is_some() {
                let cache_file_node = cache_file_node.unwrap();
                let slice_upstream_map =
                    cache_file_node.ctx_thread.get().slice_upstream_map.clone();
                let slice_upstream = slice_upstream_map.get().get(&slice_upstream_index).cloned();
                if slice_upstream.is_some() {
                    if r.ctx.get().is_slice_upstream_index_add {
                        let upstream_waits = {
                            let slice_upstream = slice_upstream.unwrap();
                            let slice_upstream = &mut *slice_upstream.get_mut();
                            r.ctx.get_mut().is_slice_upstream_index_add = false;
                            slice_upstream.is_upstream = false;
                            slice_upstream
                                .upstream_count
                                .fetch_sub(1, Ordering::Relaxed);

                            let mut upstream_waits = VecDeque::with_capacity(10);
                            swap(&mut upstream_waits, &mut slice_upstream.upstream_waits);
                            upstream_waits
                        };
                        for tx in upstream_waits {
                            let _ = tx.send(());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn proxy_cache_parse(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        let md5_str = {
            let stream_info = r.http_arg.stream_info.clone();
            let stream_info = &*stream_info.get();

            let net_core_proxy_conf = net_core_proxy::curr_conf(scc.net_curr_conf());
            log::debug!(target: "ext3",
                        "r.session_id:{}, var proxy_cache_key:{}",
                        r.session_id,
                        net_core_proxy_conf.proxy_cache_key
            );
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
            log::debug!(target: "ext3",
                "r.session_id:{}, proxy_cache_key:{}",
                r.session_id,
                proxy_cache_key
            );
            let md5_str = proxy_cache_key;

            md5_str
        };

        let md5 = md5::compute(&md5_str);
        let md5 = format!("{:x}", md5);
        let crc32 = crc32fast::hash(md5.as_bytes()) as usize;
        log::debug!(target: "ext3",
                    "r.session_id:{}, md5_str:{}, md5:{}",
                    r.session_id,
                    md5_str, md5
        );

        let mut is_local_cache_req = false;

        use crate::config::common_core;
        use crate::config::net_core;
        let net_curr_conf = scc.net_curr_conf();
        let common_core_any_conf = scc.common_core_any_conf();
        let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);
        let net_core_conf = net_core::curr_conf(&net_curr_conf);
        let common_core_conf = common_core::curr_conf(&common_core_any_conf);

        //当前可能是main  server local， ProxyCache必须到main_conf中读取
        let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;
        let md5_bytes = Bytes::from(md5.clone());
        log::debug!(target: "ext3", "r.session_id:{}, index_get_rand", r.session_id);
        let proxy_cache_name = net_core_proxy_main_conf.index_get_rand(&md5_bytes);
        let proxy_cache_rand = if proxy_cache_name.is_some() {
            log::debug!(target: "ext3", "r.session_id:{}, proxy_cache_rand", r.session_id);
            let proxy_cache = net_core_proxy_main_conf
                .proxy_cache_map
                .get(proxy_cache_name.as_ref().unwrap())
                .cloned();
            if proxy_cache.is_some() {
                proxy_cache
            } else {
                None
            }
        } else {
            None
        };

        let (proxy_cache, proxy_cache_path, proxy_cache_path_tmp) = if !net_core_proxy_conf
            .proxy_caches
            .is_empty()
        {
            log::debug!(target: "ext3", "r.session_id:{} proxy_caches.len:{}", r.session_id, net_core_proxy_conf.proxy_caches.len());
            let index = crc32 % net_core_proxy_conf.proxy_caches.len();
            log::debug!(target: "ext3", "r.session_id:{} proxy_caches index:{}", r.session_id, index);
            let is_host = {
                if !net_core_proxy_conf.proxy_hot_file.is_open {
                    false
                } else {
                    let proxy_cache = net_core_proxy_conf.proxy_caches[index].clone();

                    if net_core_proxy_main_conf.is_hot_io_percent(
                        &proxy_cache.cache_conf.name,
                        net_core_proxy_conf.proxy_hot_file.hot_io_percent,
                    ) {
                        proxy_cache.sort_hot(&r, &net_core_proxy_conf.proxy_hot_file);
                        HttpCacheFile::is_hot(&r, &proxy_cache, &md5_bytes).await
                    } else {
                        false
                    }
                }
            };
            log::debug!(target: "ext3", "is_host:{}", is_host);

            let proxy_cache = if r.local_cache_req_count <= 0
                && is_host
                && net_core_proxy_conf.proxy_caches.len() > 1
            {
                let other_index: usize = rand::thread_rng().gen();
                let other_index = other_index % net_core_proxy_conf.proxy_caches.len();
                let index = if other_index == index {
                    // is_local_cache_req = true;
                    // (index + 1) % net_core_proxy_conf.proxy_caches.len()
                    index
                } else {
                    is_local_cache_req = true;
                    other_index
                };
                let proxy_cache = net_core_proxy_conf.proxy_caches[index].clone();
                proxy_cache
            } else {
                let proxy_cache = net_core_proxy_conf.proxy_caches[index].clone();
                if net_core_proxy_main_conf.index_contains(&md5_bytes, &proxy_cache.cache_conf.name)
                {
                    proxy_cache
                } else {
                    if proxy_cache_rand.is_some() {
                        proxy_cache_rand.unwrap()
                    } else {
                        proxy_cache
                    }
                }
            };
            log::debug!(target: "ext3", "index:{}, is_local_cache_req:{}", index, is_local_cache_req);

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

            let tmpfile_id = common_core_conf.tmpfile_id.fetch_add(1, Ordering::Relaxed);
            let pid = unsafe { libc::getpid() };

            proxy_cache_path_tmp.push_str(&format!("{}_{}_{}", md5, pid, tmpfile_id));
            proxy_cache_path.push_str(&md5);
            log::debug!(target: "main",
                "r.session_id:{}, proxy_cache_path:{}, proxy_cache_path_tmp:{}",
                r.session_id,
                proxy_cache_path,
                proxy_cache_path_tmp
            );
            let proxy_cache_path_tmp = ArcString::from(proxy_cache_path_tmp);
            let proxy_cache_path = ArcString::from(proxy_cache_path);
            (Some(proxy_cache), proxy_cache_path, proxy_cache_path_tmp)
        } else {
            log::debug!(target: "ext3", "proxy_caches.len:{}", 0);
            let proxy_cache_path_tmp = ArcString::from("");
            let proxy_cache_path = ArcString::from("");
            (None, proxy_cache_path, proxy_cache_path_tmp)
        };

        let md5 = Bytes::from(md5);

        log::debug!(target: "ext3", "md5:{:?}", md5);
        let ret = net_core_proxy_main_conf.is_check.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        );
        if let Ok(false) = ret {
            log::debug!(target: "ext3", "compare_exchange");
            let scc = r.http_arg.stream_info.get().scc.clone();
            if proxy_cache.is_some() {
                let proxy_cache = proxy_cache.as_ref().unwrap();
                if net_core_proxy_main_conf.is_expires_file_timer() {
                    let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
                    proxy_cache_ctx
                        .cache_file_node_expires
                        .make_contiguous()
                        .sort_by(|a, b| {
                            a.cache_file_node_head
                                .expires_time()
                                .cmp(&b.cache_file_node_head.expires_time())
                        });
                }

                log::debug!(target: "ext3", "del_expires_cache_file");
                let _ = del_expires_cache_file(&md5, proxy_cache, scc.ms()).await;
            }
            log::debug!(target: "ext3", "del_md5");
            let _ = del_md5(scc.ms()).await;
            log::debug!(target: "ext3", "del_max_open_cache_file");
            let _ = del_max_open_cache_file(scc.ms()).await;
            net_core_proxy_main_conf
                .is_check
                .store(false, Ordering::Relaxed);
        }

        let is_request_cache = proxy_cache.is_some();
        if proxy_cache.is_none() {
            is_local_cache_req = false;
        }
        log::debug!(target: "main",
            "r.session_id:{}, is_request_cache:{}",
            r.session_id,
            is_request_cache
        );

        let rctx = &mut r.ctx.get_mut();
        let r_in = &mut rctx.r_in;

        let method_str = match &r_in.method {
            &http::Method::HEAD => http::Method::GET.as_str(),
            _ => r_in.method.as_str(),
        };

        let trie_url = format!("{}{}", method_str, r_in.uri.to_string());
        log::debug!(target: "ext3", "trie_url:{}", trie_url);

        let cache_file_slice = r.cache_file_slice;
        let http_cache_file = HttpCacheFile {
            ctx_thread: ArcRwLock::new(HttpCacheFileContext {
                cache_file_node_manage: typ2::ArcRwLockTokio::default(),
                cache_file_node_data: None,
                cache_file_node: None,
                cache_file_node_version: 0,
                cache_file_status: None,
            }),
            proxy_cache,
            cache_file_info: Arc::new(ProxyCacheFileInfo {
                directio: net_core_conf.directio,
                cache_file_slice,
                md5,
                md5_str: md5_str.into(),
                crc32,
                proxy_cache_path,
                proxy_cache_path_tmp,
                trie_url,
            }),
        };

        rctx.is_request_cache = is_request_cache;
        rctx.is_local_cache_req = is_local_cache_req;
        rctx.http_cache_file = Some(http_cache_file).into();

        Ok(())
    }

    async fn upstream_parse(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let stream_info = self.http_arg.stream_info.clone();
        let scc = stream_info.get().scc.clone();

        let upstream_connect_info = r.ctx.get().r_in.upstream_connect_info.clone();
        let upstream_connect_info = if upstream_connect_info.connect_func.is_none() {
            Arc::new(proxy_util::upstream_connect_info(&stream_info, &scc).await?)
        } else {
            stream_info.get_mut().err_status = ErrStatus::ServiceUnavailable;
            stream_info.get_mut().ups_balancer = upstream_connect_info.ups_balancer.clone();
            upstream_connect_info
        };
        let connect_func = &upstream_connect_info.connect_func;

        stream_info.get_mut().err_status = ErrStatus::ServiceUnavailable;

        let version = r.ctx.get().r_in.upstream_version;
        // let is_version1_upstream = match version {
        //     Version::HTTP_2 => false,
        //     _ => true,
        // };

        let protocol7 = connect_func.protocol7().await;
        let upstream_is_tls = connect_func.is_tls().await;

        let upstream_host = connect_func.host().await?;
        let (_, upstream_port) = host_and_port(upstream_host.as_str());
        let req_host = r.ctx.get_mut().r_in.upstream_headers.get(HOST).cloned();
        if req_host.is_none() {
            return Err(anyhow!("host nil"));
        }
        let req_host = req_host.unwrap();
        let req_host = req_host.to_str().unwrap();
        let (req_host, _) = host_and_port(req_host);

        let proxy = {
            use crate::config::net_server_http_proxy;
            let http_server_proxy_conf = net_server_http_proxy::curr_conf(scc.net_curr_conf());
            http_server_proxy_conf.proxy.clone().unwrap()
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

        let is_upstream_sendfile = match upstream_version {
            Version::HTTP_2 => false,
            Version::HTTP_3 => false,
            _ => true,
        };

        let is_protocol7_sendfile = match Protocol7::from_string(&protocol7)? {
            Protocol7::Tcp => true,
            _ => false,
        };

        let upstream_stream_fd = if is_upstream_sendfile && is_protocol7_sendfile {
            1
        } else {
            0
        };

        let is_upstream_sendfile =
            StreamStream::is_ups_sendfile(upstream_stream_fd, &scc, &stream_info);

        let upstream_scheme = if upstream_is_tls { "https" } else { "http" };
        let upstream_uri = format!(
            "{}://{}:{}{}",
            upstream_scheme,
            req_host,
            upstream_port,
            r.ctx
                .get()
                .r_in
                .upstream_uri
                .path_and_query()
                .map(|x| x.as_str())
                .unwrap_or("/")
        );

        let upstream_uri: http::Uri = upstream_uri.parse()?;

        let rctx = &mut *r.ctx.get_mut();
        let (cache_control_time, _expires_time, _expires_time_sys) =
            cache_control_time(&rctx.r_in.upstream_headers)?;
        let is_request_cache = if cache_control_time == 0
        /*|| !r_in.is_version1_upstream*/
        {
            false
        } else {
            let net_core_proxy_conf = net_core_proxy::curr_conf(scc.net_curr_conf());
            let proxy_cache_methods = &net_core_proxy_conf.proxy_cache_methods;
            if proxy_cache_methods.is_empty() {
                true
            } else {
                if proxy_cache_methods
                    .get(&rctx.r_in.method.as_str().to_ascii_lowercase())
                    .is_some()
                {
                    true
                } else {
                    false
                }
            }
        };

        log::debug!(target: "main",
                       "r.session_id:{}, is_request_cache:{}",
                       r.session_id,
                       is_request_cache
        );

        if !is_request_cache && r.local_cache_req_count > 0 {
            return Err(anyhow!("!is_request_cache && local_cache_req_count > 0"));
        }

        rctx.is_upstream_sendfile = is_upstream_sendfile;

        let r_in = &mut rctx.r_in;
        r_in.upstream_uri = upstream_uri;
        r_in.upstream_headers.remove(HOST);
        r_in.upstream_headers.remove(CONNECTION);
        r_in.upstream_version = upstream_version;
        //r_in.is_version1_upstream = is_version1_upstream;

        r_in.upstream_connect_info = upstream_connect_info;

        r_in.cache_control_time = cache_control_time;
        rctx.is_request_cache = is_request_cache;

        return Ok(());
    }

    async fn get_upstream_client(
        &mut self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<Arc<hyper::Client<HttpHyperConnector>>> {
        let client = r.ctx.get().client.clone();
        if client.is_some() {
            return Ok(client.unwrap());
        }

        let stream_info = self.http_arg.stream_info.clone();
        let scc = stream_info.get().scc.clone();

        let (upstream_connect_info, upstream_version) = {
            let rctx = r.ctx.get();
            let upstream_connect_info = rctx.r_in.upstream_connect_info.clone();
            let upstream_version = rctx.r_in.upstream_version;
            (upstream_connect_info, upstream_version)
        };

        if upstream_connect_info.is_connect_func_disable {
            return Err(anyhow!("err: connect_func nil"));
        }
        let connect_func = upstream_connect_info.connect_func.clone().unwrap();

        let hello = proxy_util::get_proxy_hello(
            upstream_connect_info.is_proxy_protocol_hello,
            &stream_info,
            &scc,
        )
        .await;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            let hello_str = general_purpose::STANDARD.encode(hello_str);

            stream_info.get_mut().upstream_protocol_hello_size = hello_str.len();
            r.ctx.get_mut().r_in.upstream_headers.insert(
                HeaderName::from_bytes(HTTP_HELLO_KEY.as_bytes())?,
                HeaderValue::from_bytes(hello_str.as_bytes())?,
            );
        }

        let protocol7 = connect_func.protocol7().await;

        let proxy = {
            use crate::config::net_server_http_proxy;
            let http_server_proxy_conf = net_server_http_proxy::curr_conf(scc.net_curr_conf());
            http_server_proxy_conf.proxy.clone().unwrap()
        };

        let client = self
            .get_client(
                r.session_id,
                upstream_version,
                connect_func,
                &proxy,
                &stream_info,
                &protocol7,
            )
            .await
            .map_err(|e| anyhow!("err:get_client => protocol7:{}, e:{}", protocol7, e))?;

        r.ctx.get_mut().client = Some(client.clone()).into();

        return Ok(client);
    }

    async fn stream_not_cache_request(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let is_slice = false;
        let http_cache_status = HttpCacheStatus::Bypass;
        let request_server = {
            let r_in = &mut r.ctx.get_mut().r_in;
            let mut request_server = Request::new(r_in.body.take().unwrap());

            *request_server.method_mut() = r_in.upstream_method.clone();
            *request_server.uri_mut() = r_in.upstream_uri.clone();
            *request_server.version_mut() = r_in.upstream_version.clone();
            *request_server.headers_mut() = r_in.upstream_headers.clone();
            *request_server.extensions_mut() = r_in.upstream_extensions.take().unwrap();
            request_server
        };

        let ups_request = HttpRequest::Request(request_server);
        self.stream_request(&r, ups_request, is_slice, http_cache_status)
            .await
    }

    async fn get_cache_file_node(
        &mut self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<(bool, i64, Option<Arc<AtomicI64>>, Option<CacheFileStatus>)> {
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
        if cache_file_node.is_some() {
            let ctx = cache_file_node.as_ref().unwrap().ctx_thread.get();
            let hcf_ctx_thread = &mut *http_cache_file.ctx_thread.get_mut();
            hcf_ctx_thread.cache_file_node_version = ctx.cache_file_node_version;
            return Ok((true, -1, None, None));
        }

        let (
            is_upstream,
            cache_file_status,
            cache_file_node_manage,
            cache_file_node_data,
            cache_file_node,
            upstream_count,
            upstream_count_drop,
        ) = http_cache_file.get_cache_file_node(r).await?;

        {
            let cache_file_node_manage_ = cache_file_node_manage.get(file!(), line!()).await;
            let hcf_ctx_thread = &mut *http_cache_file.ctx_thread.get_mut();
            hcf_ctx_thread.cache_file_node_version =
                cache_file_node_manage_.cache_file_node_version;
            hcf_ctx_thread.cache_file_node_manage = cache_file_node_manage.clone();
            hcf_ctx_thread.cache_file_status = cache_file_status.clone();
        }
        http_cache_file.ctx_thread.get_mut().cache_file_node_data = cache_file_node_data.clone();
        http_cache_file.ctx_thread.get_mut().cache_file_node = cache_file_node.clone();

        if cache_file_node.is_none() || is_upstream {
            log::trace!(target: "ext", "r.session_id:{}-{}, get_cache_file_node upstream",
                        r.session_id, r.local_cache_req_count);
            return Ok((
                false,
                upstream_count,
                upstream_count_drop,
                cache_file_status,
            ));
        }
        log::trace!(target: "ext", "r.session_id:{}-{}, get_cache_file_node cache file",
                    r.session_id, r.local_cache_req_count);
        return Ok((true, upstream_count, upstream_count_drop, cache_file_status));
    }

    async fn stream_cache_request_not_range(&mut self, r: &Arc<HttpStreamRequest>) -> Result<bool> {
        let (http_cache_file, is_304) = {
            let rctx = &mut *r.ctx.get_mut();
            if rctx.r_in.is_range {
                return Ok(false);
            }
            rctx.is_upstream = false;
            rctx.is_upstream_add = false;

            (rctx.http_cache_file.clone(), rctx.r_in.is_304)
        };

        let is_slice = false;
        let mut _http_cache_status = HttpCacheStatus::Bypass;
        let (is_ok, upstream_count, upstream_count_drop, cache_file_status) =
            self.get_cache_file_node(&r).await?;
        let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
        let ups_request: Result<Option<HttpRequest>> = async {
            if is_ok || (is_304 && cache_file_node.is_some()) {
                let cache_file_node = cache_file_node.as_ref().unwrap();
                let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                let is_full = bitmap.get().is_full();
                if !is_full {
                    log::debug!(target: "ext3",
                                "r.session_id:{}, not_get not is_full",
                                r.session_id,
                    );
                    if is_304 {
                        log::error!("r.session_id:{}, is_304 not is_full", r.session_id,);
                        return Err(anyhow!(
                            "err:r.session_id:{}, is_304 not is_full",
                            r.session_id,
                        ));
                    }
                    Ok(None)
                } else {
                    let ctx = cache_file_node.ctx_thread.get();
                    if let &CacheFileStatus::Expire = &ctx.cache_file_status {
                        log::trace!(target: "ext", "r.session_id:{}-{}, Expire file to Exist",
                                        r.session_id, r.local_cache_req_count);
                    }

                    log::trace!(target: "ext", "r.session_id:{}-{}, file_response_not_get",
                                    r.session_id, r.local_cache_req_count);

                    let request = http_cache_file.cache_file_request_not_get();
                    if request.is_none() {
                        return Ok(None);
                    }
                    _http_cache_status = HttpCacheStatus::Hit;
                    Ok(Some(HttpRequest::CacheFileRequest(request.unwrap())))
                }
            } else {
                log::debug!(target: "ext3",
                            "r.session_id:{}, miss",
                            r.session_id,
                );
                Ok(None)
            }
        }
        .await;
        let ups_request = ups_request?;
        let ups_request = if ups_request.is_some() {
            //丢弃body
            let body = {
                let rctx = &mut *r.ctx.get_mut();
                if rctx.r_in.content_length > 0 {
                    rctx.r_in.body.take()
                } else {
                    None
                }
            };

            if body.is_some() {
                let mut body = body.unwrap();
                use futures_util::StreamExt;
                loop {
                    let data = body.next().await;
                    if data.is_none() {
                        break;
                    }
                    let data = data.unwrap();
                    if data.is_err() {
                        break;
                    }
                }
            }
            ups_request.unwrap()
        } else {
            let request_server = {
                let rctx = &mut *r.ctx.get_mut();
                let r_in = &mut rctx.r_in;
                let body = r_in.body.take();
                if body.is_none() {
                    return Err(anyhow!("body.is_none()"));
                }

                let mut request_server = Request::new(body.unwrap());

                if rctx.is_local_cache_req {
                    *request_server.method_mut() = r_in.method.clone();
                    *request_server.uri_mut() = r_in.uri.clone();
                    *request_server.version_mut() = r_in.version.clone();
                    *request_server.headers_mut() = r_in.headers.clone();
                    *request_server.extensions_mut() = r_in.upstream_extensions.take().unwrap();
                    request_server
                } else {
                    *request_server.method_mut() = r_in.upstream_method.clone();
                    *request_server.uri_mut() = r_in.upstream_uri.clone();
                    *request_server.version_mut() = r_in.upstream_version.clone();
                    *request_server.headers_mut() = r_in.upstream_headers.clone();
                    *request_server.extensions_mut() = r_in.upstream_extensions.take().unwrap();
                    request_server
                }
            };

            let mut ups_request = {
                let rctx = &mut *r.ctx.get_mut();
                rctx.is_upstream = true;
                rctx.is_upstream_add = true;
                if upstream_count > rctx.max_upstream_count {
                    rctx.max_upstream_count = upstream_count;
                }
                rctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);
                request_server
            };

            let is_304 = r.ctx.get().r_in.is_304;
            _http_cache_status = self
                .check_upstream_header(
                    &r,
                    cache_file_node,
                    ups_request.headers_mut(),
                    !is_304,
                    cache_file_status,
                )
                .await?;
            HttpRequest::Request(ups_request)
        };

        self.stream_request(&r, ups_request, is_slice, _http_cache_status)
            .await?;
        Ok(true)
    }

    async fn is_last_upstream_cache(r: &Arc<HttpStreamRequest>) -> Result<bool> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;
        let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
            http_cache_file.proxy_cache.as_ref().unwrap(),
            &http_cache_file.cache_file_info.md5,
            net_core_proxy_main_conf.cache_file_node_queue.clone(),
        );
        let is_last_upstream_cache = cache_file_node_manage
            .get(file!(), line!())
            .await
            .is_last_upstream_cache;
        Ok(is_last_upstream_cache)
    }

    pub async fn set_is_last_upstream_cache(
        r: &HttpStreamRequest,
        is_last_upstream_cache: bool,
    ) -> Result<()> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;
        let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
            http_cache_file.proxy_cache.as_ref().unwrap(),
            &http_cache_file.cache_file_info.md5,
            net_core_proxy_main_conf.cache_file_node_queue.clone(),
        );
        cache_file_node_manage
            .get_mut(file!(), line!())
            .await
            .is_last_upstream_cache = is_last_upstream_cache;
        Ok(())
    }

    async fn get_or_update_cache_file_node(&mut self, r: &Arc<HttpStreamRequest>) -> Result<()> {
        let is_slice = true;
        let mut http_cache_status = HttpCacheStatus::Bypass;
        r.ctx.get_mut().is_upstream = false;
        r.ctx.get_mut().is_upstream_add = false;
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        let (is_ok, upstream_count, upstream_count_drop, _) = self.get_cache_file_node(&r).await?;
        let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
        let ups_request: Result<Option<HttpRequest>> = async {
            if is_ok {
                let cache_file_node = cache_file_node.unwrap();
                {
                    let ctx = cache_file_node.ctx_thread.get();
                    if let &CacheFileStatus::Expire = &ctx.cache_file_status {
                        log::trace!(target: "ext", "r.session_id:{}-{}, Expire file to Exist",
                                    r.session_id, r.local_cache_req_count);
                        log::trace!(target: "is_ups", "session_id:{}, Expire file to Exist", r.session_id);
                    }
                }
                let raw_content_length = cache_file_node.response_info.range.raw_content_length;
                let ret = get_http_filter_header_range(&r, raw_content_length).await;
                if ret.is_err() {
                    r.ctx.get_mut().r_in.is_load_range = false;
                    http_cache_status = HttpCacheStatus::Hit;
                    log::trace!(target: "ext", "r.session_id:{}-{}, header_range not, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(http_cache_file.cache_file_request_head()?)));
                }

                //如果保存的是错误吗，必须先响应头了
                if r.ctx.get().r_in.is_head || r.ctx.get().r_in.left_content_length <= 0 {
                    log::trace!(target: "ext", "r.session_id:{}-{}, http head, cache_file_request_head",
                                r.session_id, r.local_cache_req_count);
                    return Ok(Some(HttpRequest::CacheFileRequest(http_cache_file.cache_file_request_head()?)));
                }
                log::trace!(target: "ext", "r.session_id:{}-{}, http get wait slice start",
                           r.session_id, r.local_cache_req_count);
                return Ok(None);
            }

            let mut ups_request = {
                log::trace!(target: "ext", "r.session_id:{}-{}, get upstream head http_cache_status:{:?}",
                            r.session_id, r.local_cache_req_count, http_cache_status);
                let rctx = &mut *r.ctx.get_mut();
                rctx.is_upstream = true;
                rctx.is_upstream_add = true;
                if upstream_count > rctx.max_upstream_count {
                    rctx.max_upstream_count = upstream_count;
                }
                rctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);
                let mut ups_request = Request::builder().body(Body::default())?;
                *ups_request.method_mut() = http::Method::HEAD;
                if rctx.is_local_cache_req {
                    *ups_request.uri_mut() = rctx.r_in.uri.clone();
                    *ups_request.version_mut() = rctx.r_in.version.clone();
                    *ups_request.headers_mut() = rctx.r_in.headers.clone();
                } else {
                    *ups_request.uri_mut() = rctx.r_in.upstream_uri.clone();
                    *ups_request.version_mut() = rctx.r_in.upstream_version.clone();
                    *ups_request.headers_mut() = rctx.r_in.upstream_headers.clone();
                }
                ups_request.headers_mut().insert(
                    http::header::RANGE,
                    HeaderValue::from_bytes("bytes=0-".as_bytes())?,
                );
                ups_request
            };

            ups_request.headers_mut().remove(http::header::ETAG);
            ups_request.headers_mut().remove(http::header::LAST_MODIFIED);

            ups_request.headers_mut().remove(http::header::IF_NONE_MATCH);
            ups_request.headers_mut().remove(http::header::IF_MATCH);

            http_cache_status = if cache_file_node.is_some() {
                log::trace!(target: "ext",
                            "r.session_id:{}-{}, Expire file is_upstream",
                            r.session_id,
                            r.local_cache_req_count
                );
                log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus head Expired", r.session_id);
                HttpCacheStatus::Expired
            } else {
                log::trace!(target: "ext",
                            "r.session_id:{}-{}, create file",
                            r.session_id,
                            r.local_cache_req_count
                );
                log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus head Create", r.session_id);
               HttpCacheStatus::Create
            };

            return Ok(Some(HttpRequest::Request(ups_request)));
        } .await;

        let ups_request = ups_request?;
        if ups_request.is_none() {
            return Ok(());
        }
        let ups_request = ups_request.unwrap();
        self.stream_request(&r, ups_request, is_slice, http_cache_status)
            .await
    }

    async fn check_upstream_header(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        cache_file_node: Option<Arc<ProxyCacheFileNode>>,
        headers: &mut http::HeaderMap<HeaderValue>,
        is_304: bool,
        cache_file_status: Option<CacheFileStatus>,
    ) -> Result<HttpCacheStatus> {
        headers.remove(http::header::ETAG);
        headers.remove(http::header::LAST_MODIFIED);

        headers.remove(http::header::IF_NONE_MATCH);
        headers.remove(http::header::IF_MATCH);
        headers.remove(http::header::IF_MODIFIED_SINCE);
        headers.remove(http::header::IF_UNMODIFIED_SINCE);

        if cache_file_node.is_some() {
            log::trace!(target: "ext",
                        "r.session_id:{}-{}, Expire file is_upstream",
                        r.session_id,
                        r.local_cache_req_count
            );

            let cache_file_node = cache_file_node.as_ref().unwrap();
            let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
            let is_full = bitmap.get().is_full();

            if is_full {
                if is_304 {
                    log::trace!(target: "is_ups", "session_id:{}, ups 304 check", r.session_id);
                    let response_info = &cache_file_node.response_info;
                    headers.insert(http::header::IF_NONE_MATCH, response_info.e_tag.clone());
                    headers.insert(
                        http::header::IF_MODIFIED_SINCE,
                        response_info.last_modified.clone(),
                    );
                }
                log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus Expired", r.session_id);
                return Ok(HttpCacheStatus::Expired);
            } else {
                log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus Miss cache_file_status:{:?}", r.session_id, cache_file_status);
                match cache_file_status {
                    Some(CacheFileStatus::Expire) => {
                        r.ctx.get_mut().is_expire_to_miss = true;
                    }
                    _ => {}
                }
                return Ok(HttpCacheStatus::Miss);
            }
        }

        log::trace!(target: "ext",
                    "r.session_id:{}-{}, create file",
                    r.session_id,
                    r.local_cache_req_count
        );

        log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus Create", r.session_id);
        return Ok(HttpCacheStatus::Create);
    }

    /*
    async fn stream_create(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        client: Arc<hyper::Client<HttpHyperConnector>>,
    ) -> Result<()> {
        let mut is_slice = true;
        let mut _cache_status = HttpCacheStatus::Bypass;
        let ups_request: Result<Option<HttpRequest>> = async {
            let (left_content_length, is_request_cache) = {
                let rctx = r.ctx.get();
                (rctx.r_in.left_content_length, rctx.is_request_cache)
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
                        let hcf_ctx_thread = &mut *r.http_cache_file.ctx_thread.get_mut();
                        hcf_ctx_thread.cache_file_node_version = cache_file_node_manage_.cache_file_node_version;
                        hcf_ctx_thread.cache_file_node_manage = cache_file_node_manage.clone();
                        hcf_ctx_thread.cache_file_status = cache_file_status;
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
                        let hcf_ctx_thread = &mut *r.http_cache_file.ctx_thread.get_mut();
                        hcf_ctx_thread.cache_file_node_version = ctx.cache_file_node_version;
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
                    let rctx = &mut *r.ctx.get_mut();
                    if left_content_length < 0 && !rctx.r_in.is_get && !rctx.r_in.is_head {
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

            let rctx = &mut *r.ctx.get_mut();
            rctx.is_upstream = true;
            let mut ups_request =
                if left_content_length < 0 && !rctx.r_in.is_get && !rctx.r_in.is_head {
                    log::trace!(target: "ext", "r.session_id:{}-{}, not(get and head) upstream head",
                                r.session_id, r.local_cache_req_count);
                    is_slice = false;
                    rctx.r_in.request_server.take().unwrap()
                } else {
                    log::trace!(target: "ext", "r.session_id:{}-{}, get upstream head _cache_status:{:?}",
                                r.session_id, r.local_cache_req_count, _cache_status);
                    let mut ups_request = Request::builder().body(Body::default())?;
                    *ups_request.method_mut() = http::Method::HEAD;
                    *ups_request.uri_mut() = rctx.r_in.upstream_uri.clone();
                    *ups_request.version_mut() = rctx.r_in.upstream_version.clone();
                    *ups_request.headers_mut() = rctx.r_in.upstream_headers.clone();
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
            let rctx = &mut *r.ctx.get_mut();
            rctx.r_in.is_slice = is_slice;
            rctx.r_in.http_cache_status = _cache_status;
            rctx.is_upstream_sendfile
        }
        .await;

        self.do_stream_request(&r, client.clone(), ups_request, is_upstream_sendfile)
            .await
    }
     */

    pub async fn create_slice_request(
        &mut self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<Option<HttpRequest>> {
        let (range_start, range_end, slice_end, http_cache_file) = {
            let ctx = &mut *r.ctx.get_mut();
            ctx.slice_upstream_index = -1;
            ctx.is_slice_upstream_index_add = false;

            log::trace!(target: "ext", "r.session_id:{}-{}, left_content_length:{}",
                        r.session_id, r.local_cache_req_count, ctx.r_in.left_content_length);
            if ctx.r_in.left_content_length <= 0 {
                return Ok(None);
            }

            let range_start = ctx.r_in.curr_slice_start;
            let range_end = ctx.r_in.curr_slice_start + ctx.http_request_slice - 1;
            let slice_end = ctx.r_in.slice_end;
            if range_start % ctx.http_request_slice != 0 || range_end > ctx.r_in.slice_end {
                return Err(anyhow::anyhow!(
                    "err:curr_slice_start => curr_slice_start:{}, ctx.r_in.slice_end:{}, left_content_length:{}",
                    ctx.r_in.curr_slice_start, ctx.r_in.slice_end, ctx.r_in.left_content_length
                ));
            }
            (
                range_start,
                range_end,
                slice_end,
                ctx.http_cache_file.clone(),
            )
        };

        let slice_index = align_bitset_start_index(range_start, r.cache_file_slice)?;
        r.ctx.get_mut().r_in.curr_slice_index = slice_index;
        r.ctx.get_mut().r_in.is_slice = true;
        let is_last_upstream_cache = HttpStream::is_last_upstream_cache(&r).await?;
        let mut version = -1;
        let once_time = 1000 * 10;
        let (upstream_count, upstream_count_drop) = loop {
            if !is_last_upstream_cache {
                break (0, None);
            }

            let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
            let cache_file_node = if cache_file_node.is_none() {
                self.get_or_update_cache_file_node(&r).await?;
                let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
                if cache_file_node.is_none() {
                    break (0, None);
                }
                cache_file_node.unwrap()
            } else {
                cache_file_node.unwrap()
            };

            let slice_upstream_map = cache_file_node.ctx_thread.get().slice_upstream_map.clone();
            let request = http_cache_file.cache_file_request_get(range_start, range_end, slice_end);
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
                log::debug!(target: "main",
                    "111111 r.session_id:{}, slice_index:{}, slice upstream:{}",
                    r.session_id,
                    slice_index,
                    r.local_cache_req_count
                );

                log::trace!(target: "is_ups", "session_id:{}, slice slice_upstream_map nil slice_index:{}", r.session_id, slice_index);
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
                    log::debug!(target: "main",
                        "2222222 r.session_id:{}, slice_index:{}, slice upstream:{}",
                        r.session_id,
                        slice_index,
                        r.local_cache_req_count
                    );
                    log::trace!(target: "is_ups", "session_id:{}, slice upstream slice_index:{}", r.session_id, slice_index);
                    break (upstream_count, Some(slice_upstream.upstream_count.clone()));
                }
                version = slice_upstream.version;
                let (tx, rx) = tokio::sync::oneshot::channel();
                slice_upstream.upstream_waits.push_back(tx);
                rx
            };

            Self::cache_file_node_to_pool(&r).await?;

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

        Self::cache_file_node_to_pool(&r).await?;

        log::trace!(target: "ext", "r.session_id:{}-{}, slice upstream, slice_index:{} range_start:{}, range_end:{}, slice_end:{}",
                    r.session_id, r.local_cache_req_count, slice_index, range_start, range_end, slice_end);

        let range_end = if !is_last_upstream_cache {
            r.ctx.get_mut().is_request_cache = false;
            slice_end
        } else {
            self.get_or_update_cache_file_node(&r).await?;
            let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
            if cache_file_node.is_none() {
                r.ctx.get_mut().is_request_cache = false;
            }
            range_end
        };

        let ctx = &mut *r.ctx.get_mut();
        ctx.slice_upstream_index = slice_index as i32;
        ctx.is_slice_upstream_index_add = true;
        ctx.last_slice_upstream_index = slice_index as i32;
        if upstream_count > ctx.max_upstream_count {
            ctx.max_upstream_count = upstream_count;
        }

        if upstream_count >= 10 {
            log::warn!("r.session_id:{}-{}, upstream_count, upstream_count:{}, slice_index:{} range_start:{}, range_end:{}, slice_end:{}, url:{}",
                        r.session_id, r.local_cache_req_count, upstream_count, slice_index, range_start, range_end, slice_end, r.ctx.get().r_in.uri);
        }

        ctx.upstream_count_drop = UpstreamCountDrop::new(upstream_count_drop);

        ctx.r_in.http_cache_status = HttpCacheStatus::Miss;
        let mut request = Request::builder().body(Body::default())?;
        *request.method_mut() = http::Method::GET;
        if ctx.is_local_cache_req {
            *request.uri_mut() = ctx.r_in.uri.clone();
            *request.version_mut() = ctx.r_in.version;
            *request.headers_mut() = ctx.r_in.headers.clone();
        } else {
            *request.uri_mut() = ctx.r_in.upstream_uri.clone();
            *request.version_mut() = ctx.r_in.upstream_version;
            *request.headers_mut() = ctx.r_in.upstream_headers.clone();
        }
        request.headers_mut().insert(
            http::header::RANGE,
            HeaderValue::from_bytes(format!("bytes={}-{}", range_start, range_end).as_bytes())?,
        );
        request.headers_mut().remove(http::header::ETAG);
        request.headers_mut().remove(http::header::LAST_MODIFIED);
        request.headers_mut().remove(http::header::IF_NONE_MATCH);
        request.headers_mut().remove(http::header::IF_MATCH);
        request
            .headers_mut()
            .remove(http::header::IF_MODIFIED_SINCE);
        request
            .headers_mut()
            .remove(http::header::IF_UNMODIFIED_SINCE);
        log::trace!(target: "is_ups", "session_id:{}, HttpCacheStatus GET Miss slice_index:{}", r.session_id, slice_index);

        return Ok(Some(HttpRequest::Request(request)));
    }

    async fn stream_slice(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        ups_request: HttpRequest,
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

        self.do_stream_request(&r, ups_request).await?;
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        let cache_file_node = http_cache_file.ctx_thread.get().cache_file_node();
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
            if r.ctx.get().is_slice_upstream_index_add {
                let upstream_waits = {
                    let cache_file_node_ups = cache_file_node_ups.unwrap();
                    let cache_file_node_ups = &mut *cache_file_node_ups.get_mut();
                    r.ctx.get_mut().is_slice_upstream_index_add = false;
                    cache_file_node_ups.is_upstream = false;
                    cache_file_node_ups
                        .upstream_count
                        .fetch_sub(1, Ordering::Relaxed);

                    let mut upstream_waits = VecDeque::with_capacity(10);
                    swap(&mut upstream_waits, &mut cache_file_node_ups.upstream_waits);
                    upstream_waits
                };

                for tx in upstream_waits {
                    let _ = tx.send(());
                }
            }
        }

        let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
        let is_full = bitmap.get().is_full();
        if is_full {
            let slice_upstream_map = cache_file_node.ctx_thread.get().slice_upstream_map.clone();
            slice_upstream_map.get_mut().clear();
        }

        Self::cache_file_node_to_pool(&r).await?;

        Ok(())
    }

    pub async fn cache_file_node_to_pool(r: &Arc<HttpStreamRequest>) -> Result<()> {
        let http_cache_file = r.ctx.get().http_cache_file.clone();
        if http_cache_file.is_none() {
            return Ok(());
        }

        let (cache_file_node, cache_file_node_data) = {
            let hcf_ctx_thread = &mut *http_cache_file.ctx_thread.get_mut();
            (
                hcf_ctx_thread.cache_file_node.take(),
                hcf_ctx_thread.cache_file_node_data.take(),
            )
        };

        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            let cache_file_node_data = cache_file_node_data.unwrap();
            http_cache_file
                .cache_file_node_to_pool(&r, cache_file_node, cache_file_node_data)
                .await?;
        }

        return Ok(());
    }

    pub async fn start_upstream_request(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        ups_request: HttpRequest,
    ) -> Result<HttpResponse> {
        r.ctx.get_mut().r_in.curr_upstream_method = None;
        let ups_response = match ups_request {
            HttpRequest::Request(mut ups_request) => {
                let method = ups_request.method().clone();
                let content_length = content_length(ups_request.headers())
                    .map_err(|e| anyhow!("err:content_length =>e:{}", e))?;

                r.ctx.get_mut().r_in.curr_upstream_method = Some(method.clone());
                let is_local_cache_req = r.ctx.get().is_local_cache_req;
                if is_local_cache_req {
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

                let rx = if is_request_body_nil(&method) || content_length <= 0 {
                    log::debug!(target: "ext3", "r.session_id:{}-{}, is_request_body_nil ", r.session_id, r.local_cache_req_count);
                    *ups_request.body_mut() = Body::empty();
                    None
                } else {
                    let is_upstream_sendfile = r.ctx.get().is_upstream_sendfile;
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let r = r.clone();
                    r.http_arg.executors.clone()._start(
                        #[cfg(feature = "anyspawn-count")]
                        Some(format!("{}:{}", file!(), line!())),
                        move |_executors| async move {
                            let ret = Self::stream_to_upstream(
                                &r,
                                is_upstream_sendfile,
                                client_read,
                                upstream_write,
                            )
                            .await
                            .map_err(|e| anyhow::anyhow!("err:stream_to_upstream => e:{}", e));
                            let _ = tx.send(ret);
                            Ok(())
                        },
                    );
                    Some(rx)
                };

                let upstream_connect_flow_info = self
                    .http_arg
                    .stream_info
                    .get()
                    .upstream_connect_flow_info
                    .clone();

                let ups_response = if is_local_cache_req {
                    log::debug!(target: "ext3", "r.session_id:{}-{},  http_handle_local ups_request:{:#?}", r.session_id, r.local_cache_req_count,ups_request);
                    let upstream_connect_info = r.ctx.get_mut().r_in.upstream_connect_info.clone();
                    let upstream_connect_info = if upstream_connect_info.connect_func.is_none() {
                        None
                    } else {
                        Some(upstream_connect_info)
                    };
                    let ups_response =
                        http_handle_local(&r, self.arg.clone(), ups_request, upstream_connect_info)
                            .await
                            .map_err(|e| anyhow!("err:http_handle_local =>e:{}", e))?;
                    ups_response
                } else {
                    log::debug!(target: "ext3", "r.session_id:{}-{}, upstream ups_request:{:#?}", r.session_id, r.local_cache_req_count,ups_request);
                    let client = self.get_upstream_client(&r).await?;
                    let ups_response = client
                        .request(
                            ups_request,
                            Some(ReqArg {
                                upstream_connect_flow_info: upstream_connect_flow_info.clone(),
                            }),
                        )
                        .await
                        .map_err(|e| anyhow!("err:client.request =>e:{}", e));
                    if ups_response.is_err() {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::default())?
                    } else {
                        ups_response.unwrap()
                    }
                };

                if rx.is_some() {
                    let rx = rx.unwrap();
                    let _ret = rx.await?;
                    // if ups_response.status().is_success() {
                    //     ret?;
                    // }
                }

                let (parts, body) = ups_response.into_parts();
                HttpResponse {
                    response: Response::from_parts(parts, Body::empty()),
                    body: HttpResponseBody::Body(body),
                }
            }
            HttpRequest::CacheFileRequest(ups_request) => {
                log::debug!(target: "ext3", "r.session_id:{}, cache file response", r.session_id);
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
        r: &Arc<HttpStreamRequest>,
        ups_request: HttpRequest,
        is_slice: bool,
        http_cache_status: HttpCacheStatus,
    ) -> Result<()> {
        {
            let rctx = &mut *r.ctx.get_mut();
            rctx.r_in.is_slice = is_slice;
            rctx.r_in.http_cache_status = http_cache_status;
        }
        self.do_stream_request(r, ups_request).await
    }

    async fn do_stream_request(
        &mut self,
        r: &Arc<HttpStreamRequest>,
        ups_request: HttpRequest,
    ) -> Result<()> {
        r.ctx.get_mut().r_in.curr_request_count += 1;

        let ups_response = self.start_upstream_request(&r, ups_request).await?;
        log::debug!(target: "ext3",
            "r.session_id:{}-{}, ups_response:{:#?}",
            r.session_id,
            r.local_cache_req_count,
            ups_response.response
        );

        Self::stream_response(r, ups_response).await
    }

    pub async fn stream_response(
        r: &Arc<HttpStreamRequest>,
        ups_response: HttpResponse,
    ) -> Result<()> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        let (plugin_http_header_filter, plugin_http_body_filter) = {
            use crate::config::net_core_plugin;
            let http_core_plugin_main_conf = net_core_plugin::main_conf(scc.ms()).await;
            let plugin_http_header_filter =
                http_core_plugin_main_conf.plugin_http_header_filter.clone();
            let plugin_http_body_filter =
                http_core_plugin_main_conf.plugin_http_body_filter.clone();
            (plugin_http_header_filter, plugin_http_body_filter)
        };

        let HttpResponse {
            response: ups_response,
            body: upstream_body,
        } = ups_response;

        if let HttpResponseBody::Body(_) = &upstream_body {
            let is_request_cache = r.ctx.get().is_request_cache;
            let is_304 = r.ctx.get().r_in.is_304;
            log::trace!(target: "is_ups", "session_id:{}, check is_304 is_request_cache:{}, is_304:{}, ups_response.status():{}", r.session_id, is_request_cache, is_304, ups_response.status());
            if ups_response.status() == http::status::StatusCode::NOT_MODIFIED && is_request_cache {
                if !is_304 {
                    if update_or_create_cache_file_304(&r, &ups_response).await? {
                        log::trace!(target: "is_ups", "session_id:{}, cache_file_node is_304", r.session_id);
                    }
                    r.ctx.get_mut().r_in.is_304 = true;
                    return Ok(());
                } else {
                    return Err(anyhow!("err: 304  => session_id:{}", r.session_id));
                }
            }
        }

        Self::response_to_out(&r, ups_response).await?;
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
            &r,
            upstream_body,
            plugin_http_body_filter,
            &mut client_write_tx,
        )
        .await?;
        r.ctx.get_mut().client_write_tx = Some(client_write_tx);
        return Ok(());
    }

    async fn response_to_out(r: &Arc<HttpStreamRequest>, response: Response<Body>) -> Result<()> {
        let rctx = &mut *r.ctx.get_mut();

        let head = response.extensions().get::<hyper::AnyProxyRawHeaders>();
        let head = if head.is_some() {
            let head = head.unwrap();
            head.0 .0.clone()
        } else {
            http_respons_to_vec(&response).into()
        };
        rctx.r_out.head_upstream_size += head.len();

        //if rctx.r_out_main.is_none() {
        if log::log_enabled!(target: "ext3", log::Level::Debug) {
            log::debug!(target: "ext3",
                        "r.session_id:{}, raw head:{}",
                        r.session_id,
                        String::from_utf8_lossy(head.as_ref())
            );
        }

        rctx.r_out.head = Some(head);
        //}

        //let header_ext = if rctx.r_in.is_version1_upstream {
        let header_ext = response
            .extensions()
            .get::<hyper::AnyProxyRawHttpHeaderExt>();
        let header_ext = if header_ext.is_some() {
            log::debug!(target: "ext3", "session_id:{}-{}, Response header_ext", r.session_id, r.local_cache_req_count);
            let header_ext = header_ext.unwrap();
            header_ext.0 .0.clone()
        } else {
            log::debug!(target: "ext3", "session_id:{}-{}, nil Response header_ext", r.session_id ,r.local_cache_req_count);
            HttpHeaderExt::new(Arc::new(AtomicI64::new(0)))
        };
        // } else {
        //     HttpHeaderExt::new()
        // };
        rctx.r_out.header_ext = header_ext;

        rctx.r_out.status_upstream = response.status().clone();
        rctx.r_out.upstream_version = response.version().clone();
        rctx.r_out.upstream_headers = response.headers().clone();

        let (part, _) = response.into_parts();
        let http::response::Parts {
            status,
            version: _,
            headers,
            extensions,
            ..
        } = part;
        rctx.r_out.status = status;
        rctx.r_out.version = rctx.r_in.version;
        rctx.r_out.headers = headers;
        rctx.r_out.extensions.set(extensions);
        return Ok(());
    }

    async fn stream_to_upstream(
        r: &Arc<HttpStreamRequest>,
        is_upstream_sendfile: bool,
        client_read: hyper::body::Body,
        upstream_write: hyper::body::Sender,
    ) -> Result<()> {
        let stream_info = r.http_arg.stream_info.clone();
        let scc = r.http_arg.stream_info.get().scc.clone();
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
            &scc,
            &stream_info,
            upstream_stream,
            is_upstream_sendfile,
            false,
        )
        .await
        .map_err(|e| anyhow!("err:stream_single =>  e:{}", e));
        if let Err(e) = ret {
            if !stream_info.get().client_stream_flow_info.get().is_close() {
                return Err(e);
            }
        }
        return Ok(());
    }

    pub async fn stream_to_client(
        r: &Arc<HttpStreamRequest>,
        mut response_body: HttpResponseBody,
        plugin_http_body_filter: ArcRwLockTokio<PluginHttpFilter>,
        client_write_tx: &mut any_base::stream_channel_write::Stream,
    ) -> Result<()> {
        let mut file_cache_bytes = FileCacheBytes::new(r.page_size);
        loop {
            let body_buf = response_body_read(&r, &mut response_body).await?;
            let ret: Result<()> = async {
                let cache_file_node = {
                    let rctx = &mut *r.ctx.get_mut();
                    if !rctx.r_out.is_cache || rctx.r_out.is_cache_err {
                        return Ok(());
                    }

                    if rctx.http_cache_file.is_none() {
                        return Ok(());
                    }

                    let cache_file_node = rctx.http_cache_file.ctx_thread.get().cache_file_node();

                    if cache_file_node.is_none() {
                        r.ctx.get_mut().r_out.is_cache_err = true;
                        log::warn!(
                            "cache_file_node nil, session_id:{}-{}",
                            r.session_id,
                            r.local_cache_req_count
                        );
                        return Ok(());
                    }
                    cache_file_node.unwrap()
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
