use crate::bindings::exports::component::server::wasm_service;
use crate::service;
use crate::wasm_std;
use crate::Component;

impl wasm_service::Guest for Component {
    fn wasm_main(config: Option<String>) -> Result<wasm_std::Error, String> {
        service::wasm_main(config)
    }
    fn wasm_main_timeout(_config: Option<String>) -> Result<wasm_std::Error, String> {
        loop {
            wasm_std::sleep(10000 * 1000);
        }
    }
    fn wasm_main_ext1(_config: Option<String>) -> Result<wasm_std::Error, String> {
        loop {
            wasm_std::sleep(10000 * 1000);
        }
    }
    fn wasm_main_ext2(_config: Option<String>) -> Result<wasm_std::Error, String> {
        loop {
            wasm_std::sleep(10000 * 1000);
        }
    }
    fn wasm_main_ext3(_config: Option<String>) -> Result<wasm_std::Error, String> {
        loop {
            wasm_std::sleep(10000 * 1000);
        }
    }
}
