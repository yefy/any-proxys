use crate::wasm_std;

pub fn run(_config: Option<String>) -> Result<wasm_std::Error, String> {
    let version = wasm_std::anyproxy_version()?;
    wasm_std::out_add_headers(&vec![("server".to_string(), version)])?;
    wasm_std::out_del_headers(&vec!["expires".to_string(), "cache-control".to_string()])?;
    // let content_length = wasm_std::out_get_header("content-length")?;
    // if content_length.is_some() {
    //     let content_length = content_length.unwrap();
    //     let mut content_length = content_length.parse::<i64>().map_err(|e| e.to_string())?;
    //     content_length -= 100;
    //     wasm_std::out_add_header("content-length", &format!("{}", content_length))?;
    // }

    Ok(wasm_std::Error::Ok)
}