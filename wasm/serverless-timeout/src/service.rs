use crate::info;
use crate::wasm_std;
use lazy_static::lazy_static;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref WASM_CONF: Mutex<Option<WasmConf>> = Mutex::new(None);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error, String> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap()).map_err(|e| e.to_string())?;
    info!("run wasm_conf:{:?}", wasm_conf);
    *WASM_CONF.lock().unwrap() = Some(wasm_conf);

    wasm_std::add_timer(10 * 1000, 0,"serverless-timeout-0");
    wasm_std::add_timer(3 * 1000, 1,"serverless-timeout-1");
    wasm_std::add_timer(6 * 1000, 2,"serverless-timeout-2");
    loop {
        wasm_std::add_timer(3 * 1000, 3, "serverless-timeout-3");
        let value = wasm_std::session_recv()?;
        info!("new_timer:{}", value);
        if &value == "serverless-timeout-1" {
            wasm_std::del_timer(2);
        } else if &value == "serverless-timeout-0" {
            break;
        }
    }
    Ok(wasm_std::Error::Return)
}

pub fn wasm_main_timeout(_config: Option<String>) -> Result<wasm_std::Error, String> {
    let session_id = wasm_std::curr_session_id();
    loop {
        wasm_std::sleep(1000);
        let values = wasm_std::get_timer_timeout(1000);
        for value in values {
            info!("timeout:{}", value);
            let ret = wasm_std::session_send(session_id, &value);
            if let Err(e) = ret {
                info!("err: timeout => err:{}", e);
            }
        }
    }
}
