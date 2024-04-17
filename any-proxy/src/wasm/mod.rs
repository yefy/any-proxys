wasmtime::component::bindgen!({
    path: "../wasm/wit",
    world: "wasm-server",
    async: true,
});

pub mod wasm_host_http;
pub mod wasm_host_log;
pub mod wasm_host_socket;
pub mod wasm_host_std;
pub mod wasm_host_store;
pub mod wasm_host_websocket;

use crate::config::net_core_wasm::WasmPlugin;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::StreamConfigContext;
use crate::WasmPluginConf;
use any_base::typ::{OptionExt, Share};
use anyhow::anyhow;
use anyhow::Result;
use component::server::wasm_std;
use std::sync::Arc;
use wasmtime::component::ResourceTable;
use wasmtime::Store;
use wasmtime_wasi::preview2::{WasiCtx, WasiCtxBuilder, WasiView};

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
    wasm_plugin_conf: &WasmPluginConf,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    if wasm_plugin_conf.wasm_main_timeout_config.is_some() {
        run_wasm_plugin_timeout(wasm_plugin_conf, plugin, wasm_plugin).await
    } else {
        run_wasm_plugin_main(wasm_plugin_conf, plugin, wasm_plugin).await
    }
}

pub async fn run_wasm_plugin_timeout(
    wasm_plugin_conf: &WasmPluginConf,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    tokio::select! {
        biased;
        ret = run_wasm_plugin_main(wasm_plugin_conf, plugin.clone(), wasm_plugin) => {
            return ret;
        }
        ret = wasm_plugin_main_timeout(wasm_plugin_conf.wasm_main_timeout_config.as_ref().unwrap(), plugin, wasm_plugin) => {
            return ret;
        }
        else => {
            return Err(anyhow!("err:select"));
        }
    }
}

pub async fn run_wasm_plugin_main(
    wasm_plugin_conf: &WasmPluginConf,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    if wasm_plugin_conf.wasm_main_ext1_config.is_some()
        || wasm_plugin_conf.wasm_main_ext2_config.is_some()
        || wasm_plugin_conf.wasm_main_ext3_config.is_some()
    {
        tokio::select! {
            biased;
            ret = wasm_plugin_main(
                &wasm_plugin_conf.wasm_main_config,
                plugin.clone(),
                wasm_plugin,
            ) => {
                return ret;
            }
            ret = run_wasm_plugin_main_ext1(wasm_plugin_conf, plugin, wasm_plugin) => {
                return ret;
            }
            else => {
                return Err(anyhow!("err:select"));
            }
        }
    } else {
        wasm_plugin_main(&wasm_plugin_conf.wasm_main_config, plugin, wasm_plugin).await
    }
}

pub async fn run_wasm_plugin_main_ext1(
    wasm_plugin_conf: &WasmPluginConf,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    if wasm_plugin_conf.wasm_main_ext1_config.is_some() {
        if wasm_plugin_conf.wasm_main_ext2_config.is_some()
            || wasm_plugin_conf.wasm_main_ext3_config.is_some()
        {
            tokio::select! {
                biased;
                ret = wasm_plugin_main_ext1(
                    wasm_plugin_conf.wasm_main_ext1_config.as_ref().unwrap(),
                    plugin.clone(),
                    wasm_plugin,
                ) => {
                    return ret;
                }
                ret = run_wasm_plugin_main_ext2(wasm_plugin_conf, plugin, wasm_plugin) => {
                    return ret;
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        } else {
            wasm_plugin_main_ext1(
                wasm_plugin_conf.wasm_main_ext1_config.as_ref().unwrap(),
                plugin.clone(),
                wasm_plugin,
            )
            .await
        }
    } else {
        run_wasm_plugin_main_ext2(wasm_plugin_conf, plugin, wasm_plugin).await
    }
}

pub async fn run_wasm_plugin_main_ext2(
    wasm_plugin_conf: &WasmPluginConf,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    if wasm_plugin_conf.wasm_main_ext2_config.is_some() {
        if wasm_plugin_conf.wasm_main_ext3_config.is_some() {
            tokio::select! {
                biased;
                ret = wasm_plugin_main_ext2(
                    wasm_plugin_conf.wasm_main_ext2_config.as_ref().unwrap(),
                    plugin.clone(),
                    wasm_plugin,
                ) => {
                    return ret;
                }
                ret =  wasm_plugin_main_ext3(wasm_plugin_conf.wasm_main_ext3_config.as_ref().unwrap(), plugin, wasm_plugin) => {
                    return ret;
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        } else {
            wasm_plugin_main_ext2(
                wasm_plugin_conf.wasm_main_ext2_config.as_ref().unwrap(),
                plugin.clone(),
                wasm_plugin,
            )
            .await
        }
    } else {
        wasm_plugin_main_ext3(
            wasm_plugin_conf.wasm_main_ext3_config.as_ref().unwrap(),
            plugin,
            wasm_plugin,
        )
        .await
    }
}

pub async fn wasm_plugin_main(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.len() <= 0 {
        None
    } else {
        Some(config)
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub async fn wasm_plugin_main_timeout(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.len() <= 0 {
        None
    } else {
        Some(config)
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main_timeout(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub async fn wasm_plugin_main_ext1(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.len() <= 0 {
        None
    } else {
        Some(config)
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main_ext1(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub async fn wasm_plugin_main_ext2(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.len() <= 0 {
        None
    } else {
        Some(config)
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main_ext2(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub async fn wasm_plugin_main_ext3(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.len() <= 0 {
        None
    } else {
        Some(config)
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main_ext3(&mut store, config)
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

use crate::stream::connect;
use crate::wasm::component::server::wasm_socket;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use std::net::SocketAddr;

pub async fn get_socket_connect(
    typ: wasm_socket::SocketType,
    host: ArcString,
    address: SocketAddr,
    ssl_domain: Option<String>,
) -> Result<Arc<Box<dyn connect::Connect>>> {
    let connect = match typ {
        wasm_socket::SocketType::Tcp => {
            use crate::config::config_toml::default_tcp_config;
            use crate::tcp::connect::Connect;
            let tcp_config = default_tcp_config().pop().unwrap();
            let connect = Box::new(Connect::new(host, address, Arc::new(tcp_config)));
            let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
            connect
        }
        wasm_socket::SocketType::Ssl => {
            use crate::config::config_toml::default_tcp_config;
            use crate::ssl::connect::Connect;
            if ssl_domain.is_none() {
                return Err(anyhow::anyhow!("ssl_domain.is_none"));
            }
            let tcp_config = default_tcp_config().pop().unwrap();
            let connect = Box::new(Connect::new(
                host,
                address,
                ssl_domain.unwrap(),
                Arc::new(tcp_config),
            ));
            let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
            connect
        }
        wasm_socket::SocketType::Quic => {
            use crate::config::config_toml::default_quic_config;
            use crate::quic::connect::Connect;
            use crate::quic::endpoints;
            if ssl_domain.is_none() {
                return Err(anyhow::anyhow!("ssl_domain.is_none"));
            }
            //let address: SocketAddr = SocketAddr::from_str(&addr)?;
            let quic_config = default_quic_config().pop().unwrap();
            let endpoints = endpoints::Endpoints::new(&quic_config, false)
                .map_err(|e| wasm_err(e.to_string()))?;
            let endpoints = Arc::new(endpoints);

            let connect = Box::new(Connect::new(
                host,
                address,
                ssl_domain.unwrap(),
                endpoints,
                Arc::new(quic_config),
            ));
            let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
            connect
        }
    };
    Ok(connect)
}

pub async fn socket_connect(
    typ: wasm_socket::SocketType,
    host: ArcString,
    address: SocketAddr,
    ssl_domain: Option<String>,
) -> Result<StreamFlow> {
    let connect = get_socket_connect(typ, host, address, ssl_domain).await?;
    let (stream, _) = connect
        .connect(None, None, None)
        .await
        .map_err(|e| anyhow::anyhow!("err:connect => err:{}", e))?;
    Ok(stream)
}
