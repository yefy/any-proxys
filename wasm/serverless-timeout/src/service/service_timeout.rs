use crate::info;
use crate::wasm_std;
use anyhow::Result;

pub fn wasm_main(_config: Option<String>) -> Result<wasm_std::Error> {
    let session_id = wasm_std::main_session_id();
    loop {
        wasm_std::sleep(1000);
        let values = wasm_std::get_timer_timeout(1000);
        for (key, value) in values {
            info!("timeout:{}", unsafe {
                String::from_utf8_unchecked(value.clone())
            });
            let ret = wasm_std::session_send(session_id, key, &value);
            if let Err(e) = ret {
                info!("err: timeout => err:{}", e);
            }
        }
    }
}