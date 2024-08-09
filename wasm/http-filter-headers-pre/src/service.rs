use crate::wasm_std;
use crate::info;
use anyhow::Result;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error> {
    info!("{:?}", "http-filter-headers-pre");
    wasm_std::out_del_headers(&vec![
        "expires".to_string(),
        "cache-control".to_string(),
    ]).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(wasm_std::Error::Ok)
}