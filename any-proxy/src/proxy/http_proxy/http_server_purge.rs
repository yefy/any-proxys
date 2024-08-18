use crate::config::net_core_proxy;
use crate::proxy::http_proxy::http_stream_request::{
    HttpCacheStatus, HttpResponse, HttpResponseBody, HttpStreamRequest,
};
use crate::proxy::http_proxy::util::{del_md5, set_expires_cache_file};
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::ServerArg;
use anyhow::Result;
use bytes::Bytes;
use http::{Response, StatusCode};
use hyper::Body;
use radix_trie::TrieCommon;
use std::collections::VecDeque;
use std::sync::Arc;

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
    let scc = r.http_arg.stream_info.get().scc.clone();
    let net_core_proxy_conf = net_core_proxy::curr_conf(scc.net_curr_conf());
    if !net_core_proxy_conf.proxy_cache_purge {
        return Ok(crate::Error::Ok);
    }

    {
        let rctx = &*r.ctx.get();
        let purge = rctx
            .r_in
            .headers
            .get(http::header::HeaderName::from_bytes("purge".as_bytes())?);
        if purge.is_none() {
            return Ok(crate::Error::Ok);
        }

        let purge_path = "/purge";
        let uri = &rctx.r_in.uri;
        let path_and_query = uri.path_and_query().map(|x| x.as_str()).unwrap_or("/");

        let purge = path_and_query.find(purge_path);
        if purge.is_none() || purge.unwrap() != 0 {
            return Ok(crate::Error::Ok);
        }
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
        r.ctx.get_mut().is_request_cache = false;
        r.ctx.get_mut().r_in.http_cache_status = HttpCacheStatus::Hit;
        let stream_info = r.http_arg.stream_info.clone();
        stream_info.get_mut().add_work_time1("http_server_purge");

        let response = if self.is_purge(&r).await? {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::default())?
        } else {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::default())?
        };

        if !self.header_response.is_send() {
            use super::http_server_proxy;
            http_server_proxy::HttpStream::stream_response(
                &r,
                HttpResponse {
                    response,
                    body: HttpResponseBody::Body(Body::empty()),
                },
            )
            .await?;
        }
        return Ok(());
    }

    async fn is_purge(&mut self, r: &Arc<HttpStreamRequest>) -> Result<bool> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        let net_curr_conf = scc.net_curr_conf();
        let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);
        if !net_core_proxy_conf.proxy_cache_purge {
            return Ok(false);
        }
        let is_purge_force = {
            let rctx = &*r.ctx.get();
            let purge_force = rctx.r_in.headers.get(http::header::HeaderName::from_bytes(
                "purge_force".as_bytes(),
            )?);
            if purge_force.is_none() {
                false
            } else {
                true
            }
        };

        let (is_dir, url) = {
            let purge_path = "/purge";
            let rctx = &*r.ctx.get();
            let uri = &rctx.r_in.uri;
            log::trace!(target: "purge",
                        "r.session_id:{}, purge request uri:{}",
                        r.session_id, uri);
            let path_and_query = uri.path_and_query().map(|x| x.as_str()).unwrap_or("/");

            let purge = path_and_query.find(purge_path);
            if purge.is_none() || purge.unwrap() != 0 {
                return Ok(false);
            }
            let path_and_query = &path_and_query[purge_path.len()..];
            if path_and_query.len() > 0 && &path_and_query[0..1] != "/" {
                return Ok(false);
            }

            let path_and_query_flag = if path_and_query.len() <= 0 { "/" } else { "" };
            let scheme = uri.scheme_str();
            if scheme.is_none() {
                return Ok(false);
            }
            let scheme = scheme.unwrap();

            let authority = uri.authority();
            if authority.is_none() {
                return Ok(false);
            }
            let authority = authority.unwrap();

            let url = format!(
                "{}{}://{}{}{}",
                rctx.r_in.method.as_str(),
                scheme,
                authority.as_str(),
                path_and_query_flag,
                path_and_query
            );
            let is_dir = if path_and_query.is_empty() || path_and_query == "/" {
                false
            } else {
                if &path_and_query[path_and_query.len() - 1..] == "/" {
                    true
                } else {
                    false
                }
            };
            (is_dir, url)
        };

        log::trace!(target: "purge",
                    "r.session_id:{}, purge is_dir:{}, url:{}",
                    r.session_id, is_dir, url);

        //当前可能是main  server local， ProxyCache必须到main_conf中读取
        let net_core_proxy = net_core_proxy::main_conf(scc.ms()).await;
        let mut urls = Vec::with_capacity(10);
        let md5s = if is_dir {
            let uri_trie = &net_core_proxy.main.proxy_cache_index.get().uri_trie;
            // use radix_trie::TrieCommon;
            // if log::log_enabled!(target: "ext", log::Level::Trace) {
            //     for (url, _md5s) in uri_trie.iter() {
            //         log::trace!(target: "ext",
            //                     "r.session_id:{}, is_dir:{}, url:{},",
            //                     r.session_id, is_dir, url);
            //     }
            // }
            let value = uri_trie.get_raw_descendant(&url);
            if value.is_none() {
                None
            } else {
                let mut values = VecDeque::with_capacity(100);
                let value = value.unwrap();
                for (url, value) in value.iter() {
                    urls.push(url.clone());
                    let mut value = value
                        .get()
                        .iter()
                        .map(|data| data.clone())
                        .collect::<VecDeque<Bytes>>();
                    if log::log_enabled!(target: "purge", log::Level::Trace) {
                        for md5 in &value {
                            log::trace!(target: "purge",
                                        "r.session_id:{}, is_dir:{}, url:{}, md5:{}",
                                        r.session_id, is_dir, url, String::from_utf8_lossy(md5.as_ref()));
                        }
                    }

                    values.append(&mut value);
                }
                Some(values)
            }
        } else {
            let value = net_core_proxy
                .main
                .proxy_cache_index
                .get()
                .uri_trie
                .get(&url)
                .cloned();
            if value.is_none() {
                None
            } else {
                let value = value.unwrap();
                urls.push(url.clone());
                let value = value
                    .get()
                    .iter()
                    .map(|data| data.clone())
                    .collect::<VecDeque<Bytes>>();
                if log::log_enabled!(target: "purge", log::Level::Trace) {
                    for md5 in &value {
                        log::trace!(target: "purge", "r.session_id:{}, is_dir:{}, url:{}, md5:{}", r.session_id, is_dir, url, String::from_utf8_lossy(md5.as_ref()));
                    }
                }
                Some(value)
            }
        };

        for url in urls {
            log::trace!(target: "purge", "r.session_id:{}, remove url:{}, ", r.session_id, url);
            //net_core_proxy.main.uri_trie.get_mut().remove(&url);
        }

        if md5s.is_none() {
            return Ok(false);
        }

        let md5s = md5s.unwrap();
        if md5s.len() <= 0 {
            return Ok(false);
        }
        if !is_purge_force {
            for md5 in &md5s {
                set_expires_cache_file(&net_core_proxy.main, md5).await?;
            }
            return Ok(true);
        }
        log::trace!(target: "purge", "del_md5");
        for md5 in &md5s {
            if let Err(e) = del_md5(&net_core_proxy.main, &md5).await {
                log::error!("err:del_md5 => e:{}", e);
            }
        }

        return Ok(true);
    }
}
