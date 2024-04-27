use crate::proxy::http_proxy::http_header_parse::content_length;
use crate::proxy::http_proxy::http_hyper_connector::HttpHyperConnector;
use crate::proxy::http_proxy::HyperExecutorLocal;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::connect::Connect;
use crate::wasm::component::server::wasm_http;
use crate::wasm::wasm_socket;
use crate::wasm::{get_socket_connect, WasmHost};
use any_base::typ::Share;
use anyhow::Result;
use async_trait::async_trait;
use http::Version;
use hyper::body::HttpBody;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::Duration;

impl WasmHost {
    pub async fn get_http_client(
        &self,
        version: Version,
        connect_func: Arc<Box<dyn Connect>>,
        stream_info: Share<StreamInfo>,
    ) -> Result<Arc<hyper::Client<HttpHyperConnector>>> {
        let session_id = stream_info.get().session_id;
        let scc = stream_info.get().scc.clone();
        let addr = connect_func.addr().await?;
        let protocol7 = connect_func.protocol7().await;
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

        let http_context = {
            use crate::config::net_core;
            let net_core_conf = net_core::curr_conf(scc.net_curr_conf());
            net_core_conf.http_context.clone()
        };

        let key = format!("wasm_{}-{}-{}", protocol7, addr, is_http2);
        log::debug!(target: "main", "session_id:{}, get_client key:{}", session_id, key);
        let client = http_context.client_map.get().get(&key).cloned();
        if client.is_some() {
            return Ok(client.unwrap());
        }

        let executors = stream_info.get().executors.clone().unwrap();
        let ms = stream_info.get().scc.ms().clone();
        let request_id = stream_info.get().request_id.clone();
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
        let client = Arc::new(client);

        http_context
            .client_map
            .get_mut()
            .insert(key, client.clone());

        Ok(client)
    }
}

#[async_trait]
impl wasm_http::Host for WasmHost {
    async fn handle_http(
        &mut self,
        typ: wasm_socket::SocketType,
        req: wasm_http::Request,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<wasm_http::Response, String>> {
        let ret: Result<wasm_http::Response> = async {
            let http_req: http::Request<hyper::Body> = match req.try_into() {
                Ok(r) => r,
                Err(_) => {
                    return Ok(wasm_http::Response {
                        status: 500,
                        headers: None,
                        body: None,
                    });
                }
            };

            let domain = http_req.uri().host();
            if domain.is_none() {
                return Err(anyhow::anyhow!("domain.is_none"));
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

            let duration = tokio::time::Duration::from_millis(timeout_ms);
            use crate::util;
            let address = util::util::lookup_host(duration, &http_host).await?;

            let connect_func = get_socket_connect(
                typ,
                domain.to_string().into(),
                address,
                Some(domain.to_string()),
            )
            .await?;

            let client = self
                .get_http_client(http_req.version(), connect_func, self.stream_info.clone())
                .await?;

            //let client = hyper::Client::new();
            let http_resp =
                match tokio::time::timeout(duration, client.request(http_req, None)).await {
                    Ok(data) => data?,
                    Err(_e) => return Err(anyhow::anyhow!("timeout")),
                };

            let wasn_resp = http_response_to_wasn_response(http_resp, timeout_ms).await;
            Ok(match wasn_resp {
                Ok(r) => r,
                Err(_) => wasm_http::Response {
                    status: 500,
                    headers: None,
                    body: None,
                },
            })
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
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
    timeout_ms: u64,
) -> Result<wasm_http::Response> {
    let duration = tokio::time::Duration::from_millis(timeout_ms);
    let status = http_res.status().as_u16();
    let content_length = content_length(http_res.headers())?;
    let mut headers: Vec<(String, String)> = Vec::with_capacity(10);
    for (key, value) in http_res.headers() {
        headers.push((key.to_string(), value.to_str()?.to_string()));
    }
    let mut data = Vec::with_capacity(content_length as usize);
    loop {
        let body = match tokio::time::timeout(duration, http_res.body_mut().data()).await {
            Ok(data) => data,
            Err(_e) => return Err(anyhow::anyhow!("timeout")),
        };
        if body.is_none() {
            break;
        }
        let body = body.unwrap()?;
        data.extend_from_slice(body.to_bytes().unwrap().as_ref());
    }
    Ok(wasm_http::Response {
        status,
        headers: Some(headers),
        body: Some(data),
    })
}
