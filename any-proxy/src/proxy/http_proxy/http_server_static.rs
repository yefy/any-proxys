use crate::proxy::http_proxy::http_cache_file_node::ProxyCacheFileNode;
use crate::proxy::http_proxy::http_stream_request::{
    HttpCacheStatus, HttpResponse, HttpResponseBody, HttpStreamRequest,
};
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::ServerArg;
use any_base::io::async_write_msg::MsgReadBufFile;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use http::header::CONNECTION;
use http::Version;
use http::{HeaderValue, Response, StatusCode};
use hyper::Body;
use std::path::Path;
use std::sync::Arc;

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
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
        {
            let rctx = &mut *r.ctx.get_mut();
            rctx.is_request_cache = true;
            rctx.is_no_cache = true;
            rctx.r_in.http_cache_status = HttpCacheStatus::Hit;
        }

        let stream_info = r.http_arg.stream_info.clone();
        stream_info.get_mut().add_work_time1("http_server_static");

        let scc = self.http_arg.stream_info.get().scc.clone();
        log::trace!(target: "main", "r.request.version:{:?}", r.ctx.get().r_in.version);

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

        let is_client_sendfile_version = match &r.ctx.get().r_in.version {
            &Version::HTTP_2 => false,
            &Version::HTTP_3 => false,
            _ => true,
        };

        let socket_fd = self.http_arg.stream_info.get().server_stream_info.raw_fd;
        let (is_open_sendfile, directio, file_name) = {
            let net_core_conf = scc.net_core_conf();

            let is_open_sendfile = net_core_conf.is_open_sendfile;
            let directio = net_core_conf.directio;

            use crate::config::net_server_http_static;
            let http_server_static_conf = net_server_http_static::curr_conf(scc.net_curr_conf());
            let mut seq = "";
            let rctx = &*r.ctx.get();
            let mut name = rctx.r_in.uri.path();

            log::trace!(target: "main", "name:{}", name);
            if name.len() <= 0 || name == "/" {
                seq = "/";
                name = &http_server_static_conf.conf.index;
            }

            let file_name = format!("{}{}{}", http_server_static_conf.conf.path, seq, name);
            (is_open_sendfile, directio, file_name)
        };
        let file_name = ArcString::new(file_name);
        let file_ext = ProxyCacheFileNode::open_file(file_name.clone(), directio).await;
        let (upstream_body, mut response) = if let Err(_e) = file_ext {
            let rctx = &mut *r.ctx.get_mut();
            let response = if Path::new(file_name.as_str()).is_dir() {
                if &file_name.as_str()[file_name.len() - 1..] != "/" {
                    let url = format!("{}/", rctx.r_in.uri);

                    let mut response = Response::builder()
                        .status(StatusCode::MOVED_PERMANENTLY)
                        .body(Body::default())?;
                    response.headers_mut().insert(
                        http::header::LOCATION,
                        HeaderValue::from_bytes(url.as_bytes())?,
                    );
                    response
                } else {
                    Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .body(Body::default())?
                }
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::default())?
            };

            let upstream_body = HttpResponseBody::Body(Body::empty());
            (upstream_body, response)
        } else {
            let file_ext = file_ext.unwrap();
            let metadata = file_ext.file.get().metadata().map_err(|e| {
                anyhow!(
                    "err:file.metadata => file_name:{}, e:{}",
                    file_name.as_str(),
                    e
                )
            })?;
            let file_len = metadata.len();
            let modified = metadata.modified().map_err(|e| {
                anyhow!(
                    "err:file.metadata => file_name:{}, e:{}",
                    file_name.as_str(),
                    e
                )
            })?;

            let last_modified = httpdate::HttpDate::from(modified.clone());
            let last_modified = last_modified.to_string();
            let datetime: DateTime<Utc> = modified.into();
            let e_tag = format!("{:x}-{:x}", datetime.timestamp(), file_len);

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::default())?;

            use crate::config::net_core;
            let net_core_conf = net_core::curr_conf(scc.net_curr_conf());
            use http::header::RANGE;
            let is_range = {
                let ctx = r.ctx.get();
                ctx.r_in.headers.get(RANGE).is_some()
            };

            let is_transfer_encoding_chunked = match &r.ctx.get().r_in.version {
                &Version::HTTP_11 => true,
                _ => false,
            };

            if !net_core_conf.transfer_encoding_chunked || is_range || !is_transfer_encoding_chunked
            {
                response
                    .headers_mut()
                    .insert(http::header::CONTENT_LENGTH, HeaderValue::from(file_len));
            } else {
                response.headers_mut().insert(
                    http::header::TRANSFER_ENCODING,
                    HeaderValue::from_bytes("chunked".as_bytes())?,
                );
            }
            response.headers_mut().insert(
                "Last-Modified",
                HeaderValue::from_bytes(last_modified.as_bytes())?,
            );
            response
                .headers_mut()
                .insert("ETag", HeaderValue::from_bytes(e_tag.as_bytes())?);
            response.headers_mut().insert(
                "Content-Type",
                HeaderValue::from_bytes("text/plain".as_bytes())?,
            );

            response.headers_mut().insert(
                "Accept-Ranges",
                HeaderValue::from_bytes("bytes".as_bytes())?,
            );

            let is_sendfile = if is_client_sendfile_version
                && socket_fd > 0
                && is_open_sendfile
                && file_ext.is_sendfile()
            {
                true
            } else {
                false
            };
            r.ctx.get_mut().is_client_sendfile = is_sendfile;
            let file = Arc::new(file_ext);
            let buf_file = MsgReadBufFile::new(file.clone(), 0, file_len);
            let upstream_body = HttpResponseBody::File(Some(buf_file));
            (upstream_body, response)
        };

        {
            let rctx = &mut *r.ctx.get_mut();
            use crate::util::default_config;
            response.headers_mut().insert(
                "Server",
                HeaderValue::from_bytes(default_config::HTTP_VERSION.as_bytes())?,
            );

            response.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );

            if rctx.r_in.version == Version::HTTP_2 {
                response.headers_mut().remove(CONNECTION);
            } else if rctx.r_in.version == Version::HTTP_10 {
                response.headers_mut().insert(
                    "Connection",
                    HeaderValue::from_bytes("keep-alive".as_bytes())?,
                );
            }
            *response.version_mut() = rctx.r_in.version;
        }

        if !self.header_response.is_send() {
            use super::http_server_proxy;
            http_server_proxy::HttpStream::stream_response(
                &r,
                HttpResponse {
                    response,
                    body: upstream_body,
                },
            )
            .await?;
        }

        return Ok(());
    }
}
