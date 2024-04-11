use crate::config::net_core_plugin::PluginHttpFilter;
use crate::config::net_core_wasm::WasmPlugin;
use any_base::typ::ArcRwLockTokio;
use lazy_static::lazy_static;
use wasmtime::Store;

use crate::wasm_bind_server;
wasm_bind_server!("../wasm/wit", "../../../wasm/wasm_host.rs");

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpFilter) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_filter_headers(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!("r.session_id:{}, http_filter_headers", r.session_id);
    do_http_filter_headers(&r).await?;

    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_headers(r: &Arc<HttpStreamRequest>) -> Result<()> {
    use crate::config::http_filter::http_filter_headers;
    use crate::config::net_core_wasm;
    let (ms, net_curr_conf) = {
        let scc = r.scc.get();
        (scc.ms(), scc.net_curr_conf())
    };
    let conf = http_filter_headers::curr_conf(&net_curr_conf);

    if conf.wasm_plugin_confs.is_none() {
        return Ok(());
    }
    let wasm_plugin_confs = conf.wasm_plugin_confs.as_ref().unwrap();
    let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
    for wasm_plugin_conf in &wasm_plugin_confs.wasm {
        let wasm_plugin = net_core_wasm_conf.get_wasm_plugin(&wasm_plugin_conf.wasm_path)?;
        let plugin = WasmHost::new(r.http_arg.stream_info.clone());
        let ret = run_wasm_plugin(&wasm_plugin_conf.wasm_config, plugin, &wasm_plugin).await;
        if let Err(e) = &ret {
            log::error!("do_http_filter_headers:{}", e);
        }
    }
    Ok(())
}
