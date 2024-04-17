use crate::info;
use crate::wasm_http;
use crate::wasm_socket;
use crate::wasm_std;
use crate::wasm_websocket;

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

    let mut headers =wasm_http::Headers::new();
    headers.push(("host".to_string(), "www.example.cn".to_string()));
    headers.push(("user-agent".to_string(), "curl/7.60.0".to_string()));
    headers.push(("accept".to_string(), "*/*".to_string()));
    headers.push(("upgrade".to_string(), "websocket".to_string()));
    headers.push(("sec-websocket-version".to_string(), "13".to_string()));
    headers.push(("sec-websocket-key".to_string(), "13".to_string()));
    headers.push(("connection".to_string(), "Upgrade".to_string()));

    let request = wasm_http::Request {
        method: wasm_http::Method::Get,
        uri: "ws://www.upstream.cn:19490/".to_string(),
        headers,
        params: wasm_http::Params::new(),
        body: None,
    };
    let fd = wasm_websocket::socket_connect(wasm_socket::SocketType::Tcp, &request)?;
    let msg = wasm_websocket::socket_read(fd)?;

    info!(
        "msg:{}",
        String::from_utf8(msg.clone()).map_err(|e| e.to_string())?
    );

    let session_id = wasm_std::curr_session_id();
    wasm_websocket::socket_write(session_id, msg.as_slice())?;
    wasm_websocket::socket_flush(session_id)?;
    wasm_websocket::socket_close(session_id)?;
    Ok(wasm_std::Error::Ok)
}
