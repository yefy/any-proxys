use crate::proxy::http_proxy::http_header_parse::content_length;
use crate::proxy::http_proxy::http_hyper_connector::HttpHyperConnector;
use crate::proxy::http_proxy::HyperExecutorLocal;
use crate::wasm::component::server::wasm_http;
use crate::wasm::wasm_socket;
use crate::wasm::{get_socket_connect, wasm_err, WasmHost};
use anyhow::Result;
use async_trait::async_trait;
use hyper::body::HttpBody;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::str::FromStr;
use tokio::time::Duration;

#[async_trait]
impl wasm_http::Host for WasmHost {
    async fn handle_http(
        &mut self,
        typ: wasm_socket::SocketType,
        req: wasm_http::Request,
    ) -> wasmtime::Result<std::result::Result<wasm_http::Response, String>> {
        let http_req: http::Request<hyper::Body> = match req.try_into() {
            Ok(r) => r,
            Err(_) => {
                return Ok(Ok(wasm_http::Response {
                    status: 500,
                    headers: None,
                    body: None,
                }));
            }
        };

        let domain = http_req.uri().host();
        if domain.is_none() {
            return Ok(Err("domain.is_none".to_string()));
        }
        let domain = domain.unwrap();
        let port = http_req.uri().port_u16();
        let http_host = if port.is_none() {
            match typ {
                wasm_socket::SocketType::Tcp => {
                    format!("{}:80", domain)
                }
                wasm_socket::SocketType::Ssl => {
                    format!("{}:443", domain)
                }
                wasm_socket::SocketType::Quic => {
                    format!("{}:443", domain)
                }
            }
        } else {
            format!("{}:{}", domain, port.unwrap())
        };

        use crate::util;
        let address = util::util::lookup_host(tokio::time::Duration::from_secs(30), &http_host)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

        let connect_func = get_socket_connect(
            typ,
            domain.to_string().into(),
            address,
            Some(domain.to_string()),
        )
        .await
        .map_err(|e| wasm_err(e.to_string()))?;

        let is_http2 = match &http_req.version() {
            &hyper::http::Version::HTTP_11 => false,
            &hyper::http::Version::HTTP_2 => true,
            _ => {
                return Ok(Err(anyhow::anyhow!(
                    "err:http version not found => version:{:?}",
                    http_req.version()
                )
                .to_string()));
            }
        };

        let executors = self.stream_info.get().executors.clone().unwrap();
        let ms = self.stream_info.get().scc.ms().clone();
        let request_id = self.stream_info.get().request_id.clone();
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&ms).await;
        let http = HttpHyperConnector::new(
            request_id,
            connect_func,
            common_core_conf.session_id.clone(),
            executors.context.run_time.clone(),
        );

        let client = hyper::Client::builder()
            .executor(HyperExecutorLocal(executors.clone()))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(10))
            .http2_only(is_http2)
            //.set_host(false)
            .build(http);

        //let client = hyper::Client::new();
        let http_resp = client.request(http_req, None).await?;
        let wasn_resp = http_response_to_wasn_response(http_resp).await;
        Ok(Ok(match wasn_resp {
            Ok(r) => r,
            Err(_) => wasm_http::Response {
                status: 500,
                headers: None,
                body: None,
            },
        }))
    }
}

pub fn method_to_str(method: &wasm_http::Method) -> &'static str {
    match method {
        &wasm_http::Method::Get => "GET",
        &wasm_http::Method::Post => "POST",
        &wasm_http::Method::Put => "PUT",
        &wasm_http::Method::Delete => "DELETE",
        &wasm_http::Method::Patch => "PATCH",
        &wasm_http::Method::Head => "HEAD",
        &wasm_http::Method::Options => "OPTIONS",
    }
}

impl TryFrom<wasm_http::Request> for http::Request<hyper::Body> {
    type Error = anyhow::Error;

    fn try_from(wasm_req: wasm_http::Request) -> Result<Self, Self::Error> {
        let mut http_req = http::Request::builder()
            .method(http::Method::from_str(method_to_str(&wasm_req.method))?)
            .uri(&wasm_req.uri);

        for (key, value) in wasm_req.headers {
            http_req = http_req.header(key, value);
        }

        let body = match wasm_req.body {
            Some(b) => hyper::Body::from(b.to_vec()),
            None => hyper::Body::default(),
        };

        Ok(http_req.body(body)?)
    }
}

pub async fn http_response_to_wasn_response(
    mut http_res: http::Response<hyper::Body>,
) -> Result<wasm_http::Response> {
    let status = http_res.status().as_u16();
    let content_length = content_length(http_res.headers())?;
    let mut headers: Vec<(String, String)> = vec![];
    for (key, value) in http_res.headers() {
        headers.push((key.to_string(), value.to_str()?.to_string()));
    }
    let mut data = Vec::with_capacity(content_length as usize);
    loop {
        let body = http_res.body_mut().data().await;
        if body.is_none() {
            break;
        }
        let body = body.unwrap();
        if body.is_err() {
            break;
        }
        let body = body.unwrap();
        data.extend_from_slice(body.to_bytes().unwrap().as_ref());
    }
    Ok(wasm_http::Response {
        status,
        headers: Some(headers),
        body: Some(data),
    })
}
