use crate::info;
use crate::wasm_std;
use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    info!("run wasm_main:{:?}", "wasm_main");
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap())?;
    info!("run wasm_conf:{:?}", wasm_conf);

    // wasm_std::out_del_headers(&vec!["expires".to_string(), "cache-control".to_string()])?;
    //
    // use crate::wasm_http;
    // let request = wasm_http::Request {
    //     method: wasm_http::Method::Get,
    //     uri: "http://www.upstream.cn:19090/1.txt".to_string(),
    //     headers: wasm_http::Headers::new(),
    //     params: wasm_http::Params::new(),
    //     body: None,
    // };
    // let response = wasm_http::handle_http(&request)?;
    //
    // info!("{}", response.status);
    // if response.body.is_some() {
    //     info!(
    //         "{}",
    //         String::from_utf8(response.body.clone().unwrap()).map_err(|e| e.to_string())?
    //     );
    // }

    Ok(wasm_std::Error::Ok)
}
