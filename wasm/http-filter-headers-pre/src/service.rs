use crate::wasm_std;
use crate::info;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error, String> {
    info!("{:?}", "http-filter-headers-pre");
    wasm_std::out_del_headers(&vec![
        "expires".to_string(),
        "cache-control".to_string(),
    ])?;
    Ok(wasm_std::Error::Ok)
}