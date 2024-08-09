use crate::wasm_std;
use crate::info;
use anyhow::Result;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error> {
    info!("{:?}", "http-in-headers");
    let version = wasm_std::anyproxy_version().map_err(|e| anyhow::anyhow!("{}", e))?;
    wasm_std::in_add_headers(&vec![("user-agent".to_string(), version)]).map_err(|e| anyhow::anyhow!("{}", e))?;
    wasm_std::in_del_headers(&vec!["cache-control".to_string()]).map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(wasm_std::Error::Ok)
}