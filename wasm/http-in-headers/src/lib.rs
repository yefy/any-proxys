#[allow(warnings)]
mod bindings;
mod macros;

use crate::bindings::component::server::wasm_std;
use crate::bindings::exports::component::server::wasm_service;

struct Component;
impl wasm_service::Guest for Component {
    fn run(_config: String) -> Result<wasm_std::Error, String> {
        let version = wasm_std::anyproxy_version()?;
        wasm_std::in_add_headers(&vec![("user-agent".to_string(), version)])?;
        wasm_std::in_del_headers(&vec!["cache-control".to_string()])?;

        Ok(wasm_std::Error::Ok)
    }
}

bindings::export!(Component with_types_in bindings);
