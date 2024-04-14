#[allow(warnings)]
mod bindings;
mod macros;
mod service;
mod util;

pub use crate::bindings::component::server::wasm_http;
pub use crate::bindings::component::server::wasm_log;
pub use crate::bindings::component::server::wasm_std;
pub use crate::bindings::component::server::wasm_store;
pub use crate::bindings::component::server::wasm_tcp;
use crate::bindings::exports::component::server::wasm_service;

struct Component;
impl wasm_service::Guest for Component {
    fn run(config: Option<String>) -> Result<wasm_std::Error, String> {
        service::run(config)
    }
}

bindings::export!(Component with_types_in bindings);
