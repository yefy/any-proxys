wasmtime::component::bindgen!({
    path: "../wasm/wit",
    world: "wasm-server",
    async: true,
});

pub mod wasm_host_http;
pub mod wasm_host_log;
pub mod wasm_host_std;
pub mod wasm_host_store;
pub mod wasm_host_tcp;

use crate::config::net_core_wasm::WasmPlugin;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::StreamConfigContext;
use any_base::typ::{OptionExt, Share};
use anyhow::anyhow;
use anyhow::Result;
use component::server::wasm_std;
use std::sync::Arc;
use wasmtime::component::ResourceTable;
use wasmtime::component::*;
use wasmtime::Store;
use wasmtime_wasi::preview2::{command, WasiCtx, WasiCtxBuilder, WasiView};

#[derive(Clone)]
pub struct WasmHost {
    stream_info: Share<StreamInfo>,
    scc: OptionExt<Arc<StreamConfigContext>>,
}

impl WasmHost {
    pub fn new(stream_info: Share<StreamInfo>) -> Self {
        let scc = stream_info.get().scc.clone();
        Self { stream_info, scc }
    }
}

pub struct ServerWasiView {
    table: ResourceTable,
    ctx: WasiCtx,
    wasm_host: WasmHost,
}

impl ServerWasiView {
    pub fn new(plugin: WasmHost) -> Self {
        let table = ResourceTable::new();
        let ctx = WasiCtxBuilder::new().inherit_stdio().build();
        Self {
            table,
            ctx,
            wasm_host: plugin,
        }
    }
    pub fn wasm_host(&mut self) -> &mut WasmHost {
        &mut self.wasm_host
    }
}

impl WasiView for ServerWasiView {
    fn table(&self) -> &ResourceTable {
        &self.table
    }

    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

pub async fn run_wasm_plugin(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let mut linker = Linker::new(&wasm_plugin.engine);

    WasmServer::add_to_linker(&mut linker, ServerWasiView::wasm_host)
        .map_err(|e| anyhow!("err:WasmServer::add_to_linker => err:{}", e))?;

    // Add the command world (aka WASI CLI) to the linker
    command::add_to_linker(&mut linker)
        .map_err(|e| anyhow!("err:command::sync::add_to_linker => err:{}", e))?;

    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) = WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &linker)
        .await
        .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_run(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub fn wasm_error_to_error(wasm_err: wasm_std::Error) -> crate::Error {
    match wasm_err {
        wasm_std::Error::Ok => return crate::Error::Ok,
        wasm_std::Error::Break => return crate::Error::Break,
        wasm_std::Error::Finish => return crate::Error::Finish,
        wasm_std::Error::Error => return crate::Error::Error,
        wasm_std::Error::Return => return crate::Error::Return,
        wasm_std::Error::Ext1 => return crate::Error::Ext1,
        wasm_std::Error::Ext2 => return crate::Error::Ext2,
        wasm_std::Error::Ext3 => return crate::Error::Ext3,
    }
}

pub fn wasm_err(err: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}
