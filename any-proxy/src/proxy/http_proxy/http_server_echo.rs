use crate::proxy::http_proxy::http_stream_request::{
    HttpResponse, HttpResponseBody, HttpStreamRequest,
};
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::ServerArg;
use anyhow::Result;
use http::header::CONNECTION;
use hyper::http::header::HeaderValue;
use hyper::http::{Response, Version};
use hyper::Body;
use std::sync::Arc;

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
    let scc = r.http_arg.stream_info.get().scc.clone();
    use crate::config::net_server_http_echo;
    let net_server_http_echo_conf = net_server_http_echo::curr_conf_mut(scc.net_curr_conf());
    if net_server_http_echo_conf.body.is_none() {
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
        let scc = r.http_arg.stream_info.get().scc.clone();
        use crate::config::net_server_http_echo;
        let http_server_echo_conf = net_server_http_echo::curr_conf(scc.net_curr_conf());

        let body = http_server_echo_conf.body.clone().unwrap();
        let len = body.len();
        let body = body.into();
        let mut response = Response::new(Body::empty());
        response.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v1.0.0".as_bytes())?,
        );
        response.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        response.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes("Thu, 08 Jul 2021 09:30:53 GMT".as_bytes())?,
        );
        response.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        response
            .headers_mut()
            .insert("ETag", HeaderValue::from_bytes("60e6c5cd-e".as_bytes())?);
        response.headers_mut().insert(
            "Accept-Ranges",
            HeaderValue::from_bytes("bytes".as_bytes())?,
        );
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
        Ok(())
    }
}
