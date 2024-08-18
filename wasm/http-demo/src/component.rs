use crate::bindings::exports::component::server::wasm_service;
use crate::service;
use crate::wasm_std;
use crate::Component;

impl wasm_service::Guest for Component {
    fn wasm_main(typ: Option<i64>, config: Option<String>) -> Result<wasm_std::Error, String> {
        let typ = if typ.is_none() { 0 } else { typ.unwrap() };
        if typ == 0 {
            return service::service::wasm_main(config).map_err(|e| e.to_string());
        } else if typ == 1 {
            return service::service_timeout::wasm_main(config).map_err(|e| e.to_string());
        } else if typ == 2 {
            return service::service_uniq::wasm_main(config).map_err(|e| e.to_string());
        }
        return service::service_timeout::wasm_main(config).map_err(|e| e.to_string());
    }
}
