use crate::config::net_core_plugin::PluginHttpIn;
use crate::proxy::http_proxy::HttpStreamRequest;
use crate::wasm::run_wasm_plugin;
use crate::wasm::WasmHost;
use any_base::typ::ArcRwLockTokio;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpIn> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpIn) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_in_headers(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!(target: "main", "r.session_id:{}, http_in_headers", r.session_id);
    do_http_in_headers(&r).await?;

    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_in_headers(r: &Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!(target: "main", "r.session_id:{}, http_in_headers", r.session_id);
    use crate::config::http_in::http_in_headers;
    use crate::config::net_core_wasm;
    let conf = http_in_headers::curr_conf(r.scc.net_curr_conf());
    if conf.wasm_plugin_confs.is_none() {
        return Ok(());
    }
    let wasm_plugin_confs = conf.wasm_plugin_confs.as_ref().unwrap();
    let net_core_wasm_conf = net_core_wasm::main_conf(r.scc.ms()).await;
    for wasm_plugin_conf in &wasm_plugin_confs.wasm {
        let wasm_plugin = net_core_wasm_conf.get_wasm_plugin(&wasm_plugin_conf.wasm_path)?;
        let plugin = WasmHost::new(r.http_arg.stream_info.clone());
        let ret = run_wasm_plugin(&wasm_plugin_conf.wasm_config, plugin, &wasm_plugin).await;
        if let Err(e) = &ret {
            log::error!("do_http_in_headers:{}", e);
        }
    }
    Ok(())
}
