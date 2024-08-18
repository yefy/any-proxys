use crate::info;
use crate::wasm_std;
use anyhow::Result;

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    info!(
        "main_session_id:{}, curr_session_id:{}, service_uniq config:{:?}",
        wasm_std::main_session_id(),
        wasm_std::curr_session_id(),
        config
    );
    loop {
        let (fd, cmd, data) = wasm_std::session_recv().map_err(|e| anyhow::anyhow!("{}", e))?;
        if cmd < 0 {
            if fd > 0 {
                info!(
                    "main_session_id:{}, curr_session_id:{}, service_uniq quit session_response",
                    wasm_std::main_session_id(),
                    wasm_std::curr_session_id()
                );

                wasm_std::session_response(fd, None).map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            break;
        }

        info!(
            "main_session_id:{}, curr_session_id:{}, service_uniq data:{}",
            wasm_std::main_session_id(),
            wasm_std::curr_session_id(),
            String::from_utf8(data)?
        );

        if fd > 0 {
            let data = format!(
                "main_session_id:{}, curr_session_id:{}, service_uniq session_response ok",
                wasm_std::main_session_id(),
                wasm_std::curr_session_id(),
            );
            wasm_std::session_response(fd, Some(data.as_bytes()))
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
    }
    info!(
        "main_session_id:{}, curr_session_id:{}, service_uniq quit",
        wasm_std::main_session_id(),
        wasm_std::curr_session_id()
    );

    Ok(wasm_std::Error::Ok)
}
