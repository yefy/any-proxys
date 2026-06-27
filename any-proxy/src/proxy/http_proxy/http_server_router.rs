use crate::proxy::http_proxy::http_stream_request::{
    HttpResponse, HttpResponseBody, HttpStreamRequest,
};
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::ServerArg;
use anyhow::Result;
use http::header::CONNECTION;
use hyper::http::header::HeaderValue;
use hyper::http::{Response, Version};
use std::sync::Arc;
use std::time::SystemTime;

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
    let scc = r.http_arg.stream_info.get().scc.clone();
    use crate::config::net_server_http_router;
    let net_server_http_router_conf = net_server_http_router::curr_conf_mut(scc.net_curr_conf());
    if !net_server_http_router_conf.is_open_router {
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
        r.ctx.get_mut().is_request_cache = false;

        let stream_info = r.http_arg.stream_info.clone();
        stream_info.get_mut().add_work_time1("http_server_router");

        let scc = r.http_arg.stream_info.get().scc.clone();

        use crate::proxy::http_proxy::http_router;
        let router_req = {
            let ctx = r.ctx.get();
            http_router::Request::new(
                r.clone(),
                ctx.r_in.method.clone(),
                ctx.r_in.uri.path().to_string(),
            )
        };

        use crate::config::net_server_http_router;
        let net_server_http_router_conf = net_server_http_router::curr_conf(scc.net_curr_conf());

        let mut response = net_server_http_router_conf.router.call(router_req).await;

        if !response.headers().contains_key("Server") {
            response.headers_mut().insert(
                "Server",
                HeaderValue::from_bytes("any-proxy/v3.3.0".as_bytes())?,
            );
        }
        if !response.headers().contains_key("Content-Type") {
            response.headers_mut().insert(
                "Content-Type",
                HeaderValue::from_bytes("text/plain".as_bytes())?,
            );
        }

        if !response.headers().contains_key("Last-Modified") {
            let modified = SystemTime::now();
            let last_modified = httpdate::HttpDate::from(modified);
            let last_modified = last_modified.to_string();
            response.headers_mut().insert(
                "Last-Modified",
                HeaderValue::from_bytes(last_modified.as_bytes())?,
            );
        }

        if !response.headers().contains_key("Connection") {
            response.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }
        // response
        //     .headers_mut()
        //     .insert("ETag", HeaderValue::from_bytes(b"default")?);
        // response.headers_mut().insert(
        //     "Accept-Ranges",
        //     HeaderValue::from_bytes("bytes".as_bytes())?,
        // );
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
        let (parts, body) = response.into_parts();
        let response = Response::from_parts(parts, hyper::Body::empty());
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
        Ok(())
    }
}
