use crate::wasm_std;
//use crate::info;
use anyhow::Result;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error> {
    //info!("{:?}", "http-filter-headers");
    let version = wasm_std::anyproxy_version().map_err(|e| anyhow::anyhow!("{}", e))?;
    wasm_std::out_add_headers(&vec![("server".to_string(), version)])
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let request_id = wasm_std::variable("request_id").map_err(|e| anyhow::anyhow!("{}", e))?;
    if request_id.is_some() {
        let request_id = request_id.unwrap();
        wasm_std::out_add_header("anyproxy_request_id", &request_id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    let http_cache_file_path =
        wasm_std::variable("http_cache_file_path").map_err(|e| anyhow::anyhow!("{}", e))?;
    if http_cache_file_path.is_some() {
        let http_cache_file_path = http_cache_file_path.unwrap();
        wasm_std::out_add_header("anyproxy_http_cache_file_path", &http_cache_file_path)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    let http_cache_status =
        wasm_std::variable("http_cache_status").map_err(|e| anyhow::anyhow!("{}", e))?;
    if http_cache_status.is_some() {
        let http_cache_status = http_cache_status.unwrap();
        wasm_std::out_add_header("anyproxy_http_cache_status", &http_cache_status)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    let http_cache_file_status =
        wasm_std::variable("http_cache_file_status").map_err(|e| anyhow::anyhow!("{}", e))?;
    if http_cache_file_status.is_some() {
        let http_cache_file_status = http_cache_file_status.unwrap();
        wasm_std::out_add_header("anyproxy_http_cache_file_status", &http_cache_file_status)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    //wasm_std::out_del_headers(&vec!["expires".to_string(), "cache-control".to_string()])?;
    // let content_length = wasm_std::out_get_header("content-length")?;
    // if content_length.is_some() {
    //     let content_length = content_length.unwrap();
    //     let mut content_length = content_length.parse::<i64>().map_err(|e| e.to_string())?;
    //     content_length -= 100;
    //     wasm_std::out_add_header("content-length", &format!("{}", content_length))?;
    // }

    Ok(wasm_std::Error::Ok)
}
