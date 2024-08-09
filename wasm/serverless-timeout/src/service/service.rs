use crate::info;
use crate::wasm_std;
use lazy_static::lazy_static;
use std::sync::Mutex;
use anyhow::Result;

use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref WASM_CONF: Mutex<Option<WasmConf>> = Mutex::new(None);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn wasm_main(config: Option<String>) -> Result<wasm_std::Error> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap())?;
    info!("run wasm_conf:{:?}", wasm_conf);
    *WASM_CONF.lock().unwrap() = Some(wasm_conf);

    wasm_std::spawn(Some(1), None).map_err(|e| anyhow::anyhow!("{}", e))?;

    wasm_std::add_timer(10 * 1000, 0, b"serverless-timeout-0");
    wasm_std::add_timer(3 * 1000, 1, b"serverless-timeout-1");
    wasm_std::add_timer(6 * 1000, 2, b"serverless-timeout-2");
    loop {
        wasm_std::add_timer(3 * 1000, 3, b"serverless-timeout-3");
        let (_fd, _cmd, value) = wasm_std::session_recv().map_err(|e| anyhow::anyhow!("{}", e))?;
        info!("new_timer:{}", unsafe {
            String::from_utf8_unchecked(value.clone())
        });
        if &value == b"serverless-timeout-1" {
            wasm_std::del_timer(2);
        } else if &value == b"serverless-timeout-0" {
            break;
        }
    }
    Ok(wasm_std::Error::Return)
}