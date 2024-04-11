#[allow(warnings)]
mod bindings;
mod macros;

use crate::bindings::component::server::wasm_std;
use crate::bindings::component::server::wasm_std::Error;
use crate::bindings::exports::component::server::wasm_service;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub expires: usize,
}

struct Component;
impl wasm_service::Guest for Component {
    fn run(config: String) -> Result<Error, String> {
        let wasm_conf: WasmConf = toml::from_str(&config).map_err(|e| e.to_string())?;
        debug!("wasm_conf:{:?}", wasm_conf);

        let cache_control_key = "cache-control";
        if wasm_conf.expires > 0 && !wasm_std::out_is_header(cache_control_key)? {
            wasm_std::out_add_header(
                cache_control_key,
                &format!("max-age={}", wasm_conf.expires),
            )?;
        }
        Ok(Error::Ok)
    }
}

bindings::export!(Component with_types_in bindings);
