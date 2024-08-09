use crate::wasm_std;
use crate::debug;
use crate::info;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub expires: usize,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    info!("{:?}", "http-filter-headers-pre-expire");
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap())?;
    debug!("wasm_conf:{:?}", wasm_conf);

    let cache_control_key = "cache-control";
    if wasm_conf.expires > 0 && !wasm_std::out_is_header(cache_control_key).map_err(|e| anyhow::anyhow!("{}", e))? {
        wasm_std::out_add_header(
            cache_control_key,
            &format!("max-age={}", wasm_conf.expires),
        ).map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    Ok(wasm_std::Error::Ok)
}