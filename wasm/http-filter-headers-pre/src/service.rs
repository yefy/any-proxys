use crate::wasm_std;

pub fn run(_config: Option<String>) -> Result<wasm_std::Error, String> {
    wasm_std::out_del_headers(&vec![
        "expires".to_string(),
        "cache-control".to_string(),
    ])?;
    Ok(wasm_std::Error::Ok)
}