use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::HttpStreamRequest;
use crate::wasm::run_wasm_plugin;
use crate::wasm::WasmHost;
use any_base::typ::ArcRwLockTokio;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpFilter) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_filter_headers_pre(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!(target: "ext2", "r.session_id:{}, http_filter_headers_pre", r.session_id);
    do_http_filter_pre_headers(&r).await?;

    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_pre_headers(r: &Arc<HttpStreamRequest>) -> Result<()> {
    use crate::config::http_filter::http_filter_headers_pre;
    use crate::config::net_core_wasm;
    let stream_info = &r.http_arg.stream_info;
    let scc = stream_info.get().scc.clone();
    let conf = http_filter_headers_pre::curr_conf(scc.net_curr_conf());

    if conf.wasm_plugin_confs.is_none() {
        return Ok(());
    }
    let wasm_stream_info = stream_info.get().wasm_stream_info.clone();
    let session_id = stream_info.get().session_id.clone();
    let wasm_plugin_confs = conf.wasm_plugin_confs.as_ref().unwrap();
    let net_core_wasm_conf = net_core_wasm::main_conf(scc.ms()).await;
    for wasm_plugin_conf in &wasm_plugin_confs.wasm {
        let wasm_plugin = net_core_wasm_conf.get_wasm_plugin(&wasm_plugin_conf.wasm_path)?;
        let wasm_host = WasmHost::new(
            session_id,
            stream_info.clone(),
            wasm_plugin,
            wasm_stream_info.clone(),
        );
        let ret = run_wasm_plugin(wasm_plugin_conf, wasm_host).await;
        if let Err(e) = &ret {
            log::error!("do_http_filter_pre_headers:{}", e);
        }
    }
    Ok(())
}
