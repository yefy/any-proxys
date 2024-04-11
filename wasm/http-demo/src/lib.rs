#[allow(warnings)]
mod bindings;
mod macros;

//use crate::bindings::component::server::wasm_std;
use crate::bindings::component::server::wasm_std::Error;
use crate::bindings::exports::component::server::wasm_service;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

struct Component;
impl wasm_service::Guest for Component {
    fn run(config: String) -> Result<Error, String> {
        let wasm_conf: WasmConf = toml::from_str(&config).map_err(|e| e.to_string())?;
        info!("wasm_conf:{:?}", wasm_conf);

        // wasm_std::out_del_headers(&vec![
        //     "expires".to_string(),
        //     "cache-control".to_string(),
        // ])?;

        // let request = wasm_std::Request {
        //     method: wasm_std::Method::Get,
        //     uri: "http://www.upstream.cn:19090/1.txt".to_string(),
        //     headers: wasm_std::Headers::new(),
        //     params: wasm_std::Params::new(),
        //     body: None,
        // };
        // let response = wasm_std::handle_http(&request)?;
        //
        // info!("{}", response.status);
        // if response.body.is_some() {
        //     info!(
        //         "{}",
        //         String::from_utf8(response.body.clone().unwrap()).map_err(|e| e.to_string())?
        //     );
        // }
        Ok(Error::Ok)
    }
}

bindings::export!(Component with_types_in bindings);
