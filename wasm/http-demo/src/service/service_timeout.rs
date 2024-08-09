use crate::info;
use crate::wasm_std;
use anyhow::Result;

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    let curr_session_id = wasm_std::curr_session_id();
    info!(
        "run wasm_main_timeout curr_session_id:{}, config:{:?}",
        curr_session_id, config
    );
    let (_fd, _cmd, value) = wasm_std::session_recv().map_err(|e| anyhow::anyhow!("{}", e))?;
    info!(
        "run wasm_main_timeout session_recv curr_session_id:{}, data:{:?}",
        curr_session_id,
        String::from_utf8(value)?
    );

    let session_id = wasm_std::main_session_id();
    let data = format!("wasm_main_timeout:{}", curr_session_id);
    let data = wasm_std::session_request(session_id, 0, data.as_bytes())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    if data.is_some() {
        info!(
            "run wasm_main_timeout session_response curr_session_id:{}, data:{}",
            curr_session_id,
            String::from_utf8(data.unwrap())?
        );
    }
    Ok(wasm_std::Error::Ok)
}
