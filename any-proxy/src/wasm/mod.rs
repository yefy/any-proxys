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

use crate::config::net_core_wasm::{wasm_uniq_map, WasmPlugin, WasmUniq};
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::websocket_proxy::WebSocketStreamTrait;
use crate::proxy::StreamConfigContext;
use crate::WasmPluginConf;
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcRwLockTokio, OptionExt, Share};
use anyhow::anyhow;
use anyhow::Result;
use component::server::wasm_std;
use std::sync::Arc;
use wasmtime::component::ResourceTable;
use wasmtime::Store;
use wasmtime_wasi::preview2::{WasiCtx, WasiCtxBuilder, WasiView};

pub struct WasmStreamInfo {
    pub wasm_socket_map:
        HashMap<u64, ArcMutexTokio<any_base::io::buf_stream::BufStream<StreamFlow>>>,
    pub wasm_websocket_map: HashMap<u64, ArcMutexTokio<Box<dyn WebSocketStreamTrait>>>,

    pub wasm_session_sender: async_channel::Sender<(
        i64,
        Vec<u8>,
        Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
    )>,
    pub wasm_session_receiver: async_channel::Receiver<(
        i64,
        Vec<u8>,
        Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
    )>,
    pub wasm_session_sender_quit: async_channel::Sender<(
        i64,
        Vec<u8>,
        Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
    )>,
    pub wasm_session_receiver_quit: async_channel::Receiver<(
        i64,
        Vec<u8>,
        Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
    )>,
    pub wasm_session_response_map: HashMap<u64, tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
    pub wasm_timers: HashMap<i64, (i64, Vec<u8>)>,
}

impl WasmStreamInfo {
    pub fn new(
        channel: Option<(
            async_channel::Sender<(
                i64,
                Vec<u8>,
                Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
            )>,
            async_channel::Receiver<(
                i64,
                Vec<u8>,
                Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
            )>,
        )>,
    ) -> Self {
        let (wasm_session_sender, wasm_session_receiver) = if channel.is_some() {
            channel.unwrap()
        } else {
            async_channel::bounded(1000)
        };

        let (wasm_session_sender_quit, wasm_session_receiver_quit) = async_channel::bounded(10);
        Self {
            wasm_socket_map: HashMap::new(),
            wasm_websocket_map: HashMap::new(),
            wasm_session_sender,
            wasm_session_receiver,
            wasm_session_response_map: HashMap::new(),
            wasm_timers: HashMap::new(),
            wasm_session_sender_quit,
            wasm_session_receiver_quit,
        }
    }
}

#[derive(Clone)]
pub struct WasmHost {
    session_id: u64,
    stream_info: Share<StreamInfo>,
    scc: OptionExt<Arc<StreamConfigContext>>,
    wasm_plugin: Arc<WasmPlugin>,
    wasm_stream_info_map: ArcRwLock<HashMap<u64, Share<WasmStreamInfo>>>,
    r: OptionExt<Arc<HttpStreamRequest>>,
    //自己独有的
    wasm_stream_info: Share<WasmStreamInfo>,
    pub wasm_session_sender_uniq: OptionExt<
        async_channel::Sender<(
            i64,
            Vec<u8>,
            Option<tokio::sync::oneshot::Sender<Option<Vec<u8>>>>,
        )>,
    >,
}

impl WasmHost {
    pub fn new(
        session_id: u64,
        stream_info: Share<StreamInfo>,
        wasm_plugin: Arc<WasmPlugin>,
        wasm_stream_info: Share<WasmStreamInfo>,
    ) -> Self {
        let r = stream_info.get().http_r.clone();
        let scc = stream_info.get().scc.clone();
        let wasm_stream_info_map = stream_info.get().wasm_stream_info_map.clone();
        Self {
            session_id,
            stream_info,
            wasm_plugin,
            scc,
            wasm_stream_info,
            wasm_stream_info_map,
            r,
            wasm_session_sender_uniq: None.into(),
        }
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
    wasm_host: WasmHost,
) -> Result<crate::Error> {
    let config = &wasm_plugin_conf.wasm_main_config;
    let stream_info = &wasm_host.stream_info.clone();
    tokio::select! {
        biased;
        ret = do_run_wasm_plugin(None, config, wasm_host) => {
            return ret;
        }
        _ = wasm_wait_spawn(stream_info) => {
            return Ok(crate::Error::Error);
        }
        else => {
            return Ok(crate::Error::Error);
        }
    }
}
/*
pub async fn do_run_wasm_plugin(
    tye: Option<i64>,
    config: &Option<String>,
    plugin: WasmHost,
) -> Result<crate::Error> {
    let wasm_stream_info = plugin.wasm_stream_info.clone();
    let plugin_spawn = plugin.clone();
    tokio::select! {
        biased;
        ret = run_wasm(tye, config, plugin) => {
            return ret;
        }
        _ = check_timer_timeout(1000, &wasm_stream_info) => {
            return Ok(crate::Error::Error);
        }
        _ = wasm_wait_spawn(plugin_spawn) => {
            return Ok(crate::Error::Error);
        }
        else => {
            return Ok(crate::Error::Error);
        }
    }
}

pub async fn wasm_wait_spawn(plugin: WasmHost) -> Result<()> {
    let executors = plugin.stream_info.get().executors.clone().unwrap();
    let mut wasm_spawn_receiver = plugin.wasm_stream_info.get().wasm_spawn_receiver.clone();
    loop {
        let (tye, config, wasm_host) = wasm_spawn_receiver
            .recv()
            .await
            .map_err(|e| anyhow!("err:{:?}", e))?;
        executors.clone()._start_and_free(move |_| async move {
            let session_id = wasm_host.session_id;
            let wasm_stream_info_map = wasm_host.wasm_stream_info_map.clone();
            if let Err(e) = do_run_wasm_plugin(tye, &config, wasm_host).await {
                log::error!("err:do_run_wasm_plugin => e:{}", e);
            }
            wasm_stream_info_map.get_mut().remove(&session_id);
            Ok(())
        });
    }
}

 */

pub async fn do_run_wasm_plugin(
    tye: Option<i64>,
    config: &Option<String>,
    wasm_host: WasmHost,
) -> Result<crate::Error> {
    let wasm_stream_info = wasm_host.wasm_stream_info.clone();
    tokio::select! {
        biased;
        ret = run_wasm(tye, config, wasm_host) => {
            return ret;
        }
        _ = check_timer_timeout(1000, &wasm_stream_info) => {
            return Ok(crate::Error::Error);
        }
        else => {
            return Ok(crate::Error::Error);
        }
    }
}

pub async fn wasm_wait_spawn(stream_info: &Share<StreamInfo>) -> Result<()> {
    let executors = stream_info.get().executors.clone().unwrap();
    let wasm_spawn_receiver = stream_info.get().wasm_spawn_receiver.clone();
    loop {
        let (tye, config, wasm_host) = wasm_spawn_receiver
            .recv()
            .await
            .map_err(|e| anyhow!("err:{:?}", e))?;
        executors.clone()._start_and_free(move |_| async move {
            let session_id = wasm_host.session_id;
            let wasm_stream_info_map = wasm_host.wasm_stream_info_map.clone();
            if let Err(e) = do_run_wasm_plugin(tye, &config, wasm_host).await {
                log::error!("err:do_run_wasm_plugin => e:{}", e);
            }
            wasm_stream_info_map.get_mut().remove(&session_id);
            Ok(())
        });
    }
}

pub async fn run_wasm(
    typ: Option<i64>,
    config: &Option<String>,
    plugin: WasmHost,
) -> Result<crate::Error> {
    let wasm_plugin = plugin.wasm_plugin.clone();
    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) =
        WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &wasm_plugin.linker)
            .await
            .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    let config = if config.is_none() {
        None
    } else {
        Some(config.as_ref().unwrap().as_str())
    };

    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_wasm_main(&mut store, typ, config)
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

use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::stream::connect;
use crate::wasm::component::server::wasm_socket;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use std::collections::HashMap;
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

async fn get_timer_timeout(
    time_ms: u64,
    wasm_stream_info: &Share<WasmStreamInfo>,
) -> wasmtime::Result<Vec<(i64, Vec<u8>)>> {
    let wasm_stream_info = &mut *wasm_stream_info.get_mut();
    let mut expire_keys = Vec::with_capacity(10);
    for (cmd, (_time_ms, _)) in &mut wasm_stream_info.wasm_timers.iter_mut() {
        *_time_ms -= time_ms as i64;
        if *_time_ms <= 0 {
            expire_keys.push(*cmd);
        }
    }

    let mut values = Vec::with_capacity(10);
    for cmd in expire_keys {
        let timer = wasm_stream_info.wasm_timers.remove(&cmd);
        if timer.is_none() {
            continue;
        }
        let (_, value) = timer.unwrap();
        values.push((cmd, value));
    }

    Ok(values)
}

async fn check_timer_timeout(
    time_ms: u64,
    wasm_stream_info: &Share<WasmStreamInfo>,
) -> wasmtime::Result<()> {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(time_ms)).await;
        let values = get_timer_timeout(time_ms, wasm_stream_info).await?;
        for (key, value) in values {
            let wasm_session_sender = wasm_stream_info.get().wasm_session_sender.clone();
            let _ = wasm_session_sender.send((key, value, None)).await;
        }
    }
}

async fn session_send(
    wasm_stream_info_map: &ArcRwLock<HashMap<u64, Share<WasmStreamInfo>>>,
    session_id: u64,
    cmd: i64,
    value: Vec<u8>,
) -> wasmtime::Result<std::result::Result<(), String>> {
    let ret: Result<()> = async {
        let wasm_stream_info = wasm_stream_info_map.get().get(&session_id).cloned();
        if wasm_stream_info.is_none() {
            return Err(anyhow::anyhow!("wasm_stream_info.is_none"));
        }
        let wasm_stream_info = wasm_stream_info.unwrap();
        let wasm_session_sender = wasm_stream_info.get().wasm_session_sender.clone();
        wasm_session_sender.send((cmd, value, None)).await?;
        Ok(())
    }
    .await;
    Ok(ret.map_err(|e| e.to_string()))
}

pub fn get_wasm_uniq(name: String) -> ArcRwLockTokio<WasmUniq> {
    let wasm_uniq_map = wasm_uniq_map();
    let wash_uniq = wasm_uniq_map.get().get(&name).cloned();
    let wash_uniq = if wash_uniq.is_some() {
        wash_uniq.unwrap()
    } else {
        let wasm_uniq_map = &mut wasm_uniq_map.get_mut();
        let wash_uniq = wasm_uniq_map.get(&name).cloned();
        if wash_uniq.is_some() {
            wash_uniq.unwrap()
        } else {
            let wash_uniq = ArcRwLockTokio::new(WasmUniq::new());
            wasm_uniq_map.insert(name, wash_uniq.clone());
            wash_uniq
        }
    };
    wash_uniq
}
