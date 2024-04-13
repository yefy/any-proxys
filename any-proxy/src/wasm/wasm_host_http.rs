use crate::proxy::http_proxy::http_header_parse::content_length;
use crate::wasm::component::server::wasm_http;
use crate::wasm::WasmHost;
use anyhow::Result;
use async_trait::async_trait;
use hyper::body::HttpBody;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::str::FromStr;

#[async_trait]
impl wasm_http::Host for WasmHost {
    async fn handle_http(
        &mut self,
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

        let client = hyper::Client::new();
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
