use crate::bindings::exports::component::server::wasm_service;
use crate::service;
use crate::wasm_std;
use crate::Component;

impl wasm_service::Guest for Component {
    fn wasm_main(_typ: Option<i64>, config: Option<String>) -> Result<wasm_std::Error, String> {
        service::wasm_main(config).map_err(|e| e.to_string())
    }
}
