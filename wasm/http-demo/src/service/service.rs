use crate::info;
use crate::wasm_http;
use crate::wasm_std;
use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    info!("run wasm_main:{:?}", config);
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf =
        toml::from_str(&config.unwrap()).map_err(|e| anyhow::anyhow!("{}", e))?;
    info!("run wasm_conf:{:?}", wasm_conf);

    for i in 0..5 {
        let session_id = wasm_std::spawn(Some(1), None).map_err(|e| anyhow::anyhow!("{}", e))?;
        info!("run spawn i:{}, fd:{}", i, session_id);
        let data = format!("wasm_main:{}", i);
        wasm_std::session_send(session_id, 0, data.as_bytes())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    for i in 0..5 {
        let (session_id, _cmd, value) =
            wasm_std::session_recv().map_err(|e| anyhow::anyhow!("{}", e))?;
        info!(
            "run session_recv i:{}, data:{:?}",
            i,
            String::from_utf8(value)?
        );
        if session_id > 0 {
            wasm_std::session_response(session_id, Some("ok".as_bytes()))
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
    }

    // let body = b"Hello, http-serverless!";
    // let mut headers = wasm_http::Headers::new();
    // headers.push(("Content-Type".to_string(), "text/plain".to_string()));
    // headers.push(("Content-Length".to_string(), format!("{}", body.len())));
    // headers.push(("Connection".to_string(), "keep-alive".to_string()));
    // wasm_std::out_set_response(&wasm_http::Response {
    //     status: 200,
    //     headers: Some(headers),
    //     body: Some(body.to_vec()),
    // })
    // .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut headers = wasm_http::Headers::new();
    headers.push(("Content-Type".to_string(), "text/plain".to_string()));
    headers.push(("Content-Length".to_string(), "0".to_string()));
    headers.push(("Connection".to_string(), "keep-alive".to_string()));
    wasm_std::out_set_response(&wasm_http::Response {
        status: 404,
        headers: Some(headers),
        body: None,
    })
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(wasm_std::Error::Ok)
}
