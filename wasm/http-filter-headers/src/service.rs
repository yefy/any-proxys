use crate::wasm_std;
use crate::info;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error, String> {
    info!("{:?}", "http-filter-headers");
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