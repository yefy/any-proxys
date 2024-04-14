use crate::wasm_std;
use crate::debug;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub expires: usize,
}

pub fn run(config: Option<String>) -> Result<wasm_std::Error, String> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap()).map_err(|e| e.to_string())?;
    debug!("wasm_conf:{:?}", wasm_conf);

    let cache_control_key = "cache-control";
    if wasm_conf.expires > 0 && !wasm_std::out_is_header(cache_control_key)? {
        wasm_std::out_add_header(
            cache_control_key,
            &format!("max-age={}", wasm_conf.expires),
        )?;
    }
    Ok(wasm_std::Error::Ok)
}
