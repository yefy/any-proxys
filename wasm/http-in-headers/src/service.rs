use crate::wasm_std;
use crate::info;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error, String> {
    info!("{:?}", "http-in-headers");
    let version = wasm_std::anyproxy_version()?;
    wasm_std::in_add_headers(&vec![("user-agent".to_string(), version)])?;
    wasm_std::in_del_headers(&vec!["cache-control".to_string()])?;

    Ok(wasm_std::Error::Ok)
}