#[allow(warnings)]
mod bindings;
mod macros;

use crate::bindings::component::server::wasm_std;
use crate::bindings::component::server::wasm_std::Error;
use crate::bindings::exports::component::server::wasm_service;

struct Component;
impl wasm_service::Guest for Component {
    fn run(_config: String) -> Result<Error, String> {
        let version = wasm_std::anyproxy_version()?;
        wasm_std::in_add_headers(&vec![("user-agent".to_string(), version)])?;
        wasm_std::in_del_headers(&vec!["cache-control".to_string()])?;

        Ok(Error::Ok)
    }
}

bindings::export!(Component with_types_in bindings);
