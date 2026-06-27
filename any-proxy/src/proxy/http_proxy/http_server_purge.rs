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
use hyper::header::CONNECTION;
use hyper::http::HeaderValue;
use hyper::{Body, Version};
use radix_trie::TrieCommon;
use rivetx_core::rivetx_string::RivetxString;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::SystemTime;

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

        let (is_ok, body) = self.is_purge(&r).await?;
        let body = serde_json::to_string(&body).unwrap_or("serde_json error".to_string());
        let len = body.len();
        let body = body.to_string().into();
        let mut response = if is_ok {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::default())?
        } else {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::default())?
        };

        response.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v1.0.0".as_bytes())?,
        );
        response.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        {
            let modified = SystemTime::now();
            let last_modified = httpdate::HttpDate::from(modified);
            let last_modified = last_modified.to_string();
            response.headers_mut().insert(
                "Last-Modified",
                HeaderValue::from_bytes(last_modified.as_bytes())?,
            );
        }

        response.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        // response
        //     .headers_mut()
        //     .insert("ETag", HeaderValue::from_bytes(b"default")?);
        // response.headers_mut().insert(
        //     "Accept-Ranges",
        //     HeaderValue::from_bytes("bytes".as_bytes())?,
        // );
        response
            .headers_mut()
            .insert(http::header::CONTENT_LENGTH, HeaderValue::from(len));

        let version = r.ctx.get().r_in.version;

        if version == Version::HTTP_2 {
            response.headers_mut().remove(CONNECTION);
        }

        if version == Version::HTTP_10 {
            response.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        *response.version_mut() = version;

        if !self.header_response.is_send() {
            use super::http_server_proxy;
            http_server_proxy::HttpStream::stream_response(
                &r,
                HttpResponse {
                    response,
                    body: HttpResponseBody::Body(body),
                },
            )
            .await?;
        }
        return Ok(());
    }

    async fn is_purge(
        &mut self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<(bool, HashMap<RivetxString, serde_json::Value>)> {
        let mut body: HashMap<RivetxString, serde_json::Value> = HashMap::with_capacity(10);
        let scc = r.http_arg.stream_info.get().scc.clone();
        let net_curr_conf = scc.net_curr_conf();
        let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);
        if !net_core_proxy_conf.proxy_cache_purge {
            body.insert(
                RivetxString::from("error"),
                serde_json::json!("please open purge"),
            );
            return Ok((false, body));
        }
        let is_purge_force = {
            let rctx = &*r.ctx.get();
            let purge_force = rctx.r_in.headers.get(http::header::HeaderName::from_bytes(
                "purge_force".as_bytes(),
            )?);
            let mut is_purge_force = if purge_force.is_none() { false } else { true };
            if !is_purge_force {
                let purge = rctx
                    .r_in
                    .headers
                    .get(http::header::HeaderName::from_bytes("purge".as_bytes())?);
                if purge.is_some() {
                    let purge = purge.as_ref().unwrap();
                    let value = purge.to_str().unwrap_or("").trim();
                    if value == "purge_force" {
                        is_purge_force = true;
                    }
                }
            }
            is_purge_force
        };
        body.insert(
            RivetxString::from("is_purge_force"),
            serde_json::json!(is_purge_force),
        );

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
                body.insert(
                    RivetxString::from("error"),
                    serde_json::json!("not find /purge"),
                );
                return Ok((false, body));
            }
            let path_and_query = &path_and_query[purge_path.len()..];
            if path_and_query.len() > 0 && &path_and_query[0..1] != "/" {
                body.insert(
                    RivetxString::from("error"),
                    serde_json::json!("not find /purge/"),
                );
                return Ok((false, body));
            }

            let path_and_query_flag = if path_and_query.len() <= 0 { "/" } else { "" };
            let scheme = uri.scheme_str();
            if scheme.is_none() {
                body.insert(
                    RivetxString::from("error"),
                    serde_json::json!("scheme is_none"),
                );
                return Ok((false, body));
            }
            let scheme = scheme.unwrap();

            let authority = uri.authority();
            if authority.is_none() {
                body.insert(
                    RivetxString::from("error"),
                    serde_json::json!("authority is_none"),
                );
                return Ok((false, body));
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

        body.insert(RivetxString::from("is_dir"), serde_json::json!(is_dir));
        body.insert(RivetxString::from("url"), serde_json::json!(url));

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

        for url in &urls {
            log::trace!(target: "purge", "r.session_id:{}, remove url:{}, ", r.session_id, url);
            //net_core_proxy.main.uri_trie.get_mut().remove(&url);
        }
        body.insert(RivetxString::from("urls"), serde_json::json!(urls));

        if md5s.is_none() {
            body.insert(
                RivetxString::from("error"),
                serde_json::json!("find md5s nil"),
            );
            return Ok((false, body));
        }

        let md5s = md5s.unwrap();
        if md5s.len() <= 0 {
            body.insert(
                RivetxString::from("error"),
                serde_json::json!("find md5s len <= 0"),
            );
            return Ok((false, body));
        }

        {
            let md5s = md5s
                .iter()
                .map(|v| String::from_utf8(v.as_ref().to_vec()).unwrap_or("".to_string()))
                .collect::<Vec<String>>();
            body.insert(RivetxString::from("md5s"), serde_json::json!(md5s));
        }

        if !is_purge_force {
            for md5 in &md5s {
                set_expires_cache_file(&net_core_proxy.main, md5).await?;
            }
            return Ok((true, body));
        }
        log::trace!(target: "purge", "del_md5");
        for md5 in &md5s {
            if let Err(e) = del_md5(&net_core_proxy.main, &md5).await {
                log::error!("err:del_md5 => e:{}", e);
            }
        }

        return Ok((true, body));
    }
}
