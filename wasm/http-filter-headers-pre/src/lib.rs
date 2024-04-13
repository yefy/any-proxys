#[allow(warnings)]
mod bindings;
mod macros;

use crate::bindings::component::server::wasm_std;
use crate::bindings::exports::component::server::wasm_service;

struct Component;
impl wasm_service::Guest for Component {
    fn run(_config: String) -> Result<wasm_std::Error, String> {
        wasm_std::out_del_headers(&vec![
            "expires".to_string(),
            "cache-control".to_string(),
        ])?;
        Ok(wasm_std::Error::Ok)
    }
}

bindings::export!(Component with_types_in bindings);
