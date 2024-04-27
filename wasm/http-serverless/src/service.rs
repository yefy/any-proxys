use crate::wasm_http;
use crate::wasm_std;
use crate::wasm_socket;
use crate::info;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error, String> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap()).map_err(|e| e.to_string())?;
    info!("wasm_conf:{:?}", wasm_conf);

    let request = wasm_http::Request {
        method: wasm_http::Method::Get,
        uri: "http://www.upstream.cn:19090/1.txt".to_string(),
        headers: wasm_http::Headers::new(),
        params: wasm_http::Params::new(),
        body: None,
    };
    let timeout_ms = 1000 * 10;
    let response = wasm_http::handle_http(wasm_socket::SocketType::Tcp, &request, timeout_ms)?;

    info!("response.status:{}", response.status);
    if response.body.is_some() {
        info!(
            "response.body:{}",
            String::from_utf8(response.body.clone().unwrap()).map_err(|e| e.to_string())?
        );
    }

    let request = wasm_std::in_get_request()?;
    info!("request:{:?}", request);

    let body = b"Hello, http-serverless!";
    let mut headers = wasm_http::Headers::new();
    headers.push(("Content-Type".to_string(), "text/plain".to_string()));
    headers.push(("Content-Length".to_string(), format!("{}", body.len())));
    headers.push(("Connection".to_string(), "close".to_string()));
    wasm_std::out_set_response(&wasm_http::Response {
        status: 200,
        headers: Some(headers),
        body: Some(body.to_vec()),
    })?;
    Ok(wasm_std::Error::Ok)
}