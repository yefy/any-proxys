use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcRwLock, ArcUnsafeAny, Share};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use wasmtime::component::Component;
use wasmtime::{Config, Engine};

use crate::proxy::stream_info::StreamInfo;
use crate::wasm::ServerWasiView;
use crate::wasm::WasmServer;
use wasmtime::component::*;
use wasmtime_wasi::preview2::command;

#[derive(Clone)]
pub enum WasmHashValue {
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),
}

lazy_static! {
    pub static ref WASH_HASH: ArcRwLock<HashMap<String, WasmHashValue>> =
        ArcRwLock::new(HashMap::new());
    pub static ref WASH_HASH_HASH: ArcRwLock<HashMap<String, ArcRwLock<HashMap<String, WasmHashValue>>>> =
        ArcRwLock::new(HashMap::new());
    pub static ref WASM_STREAM_INFO_MAP: ArcRwLock<HashMap<u64, Share<StreamInfo>>> =
        ArcRwLock::new(HashMap::new());
}

pub struct WasmPlugin {
    pub path: String,
    pub engine: wasmtime::Engine,
    pub component: wasmtime::component::Component,
    pub linker: Linker<ServerWasiView>,
}

pub struct Conf {
    pub wasm_plugin_map: ArcRwLock<HashMap<String, ArcRwLock<VecDeque<Arc<WasmPlugin>>>>>,

    pub wash_hash: ArcRwLock<HashMap<String, WasmHashValue>>,
    pub wash_hash_hash: ArcRwLock<HashMap<String, ArcRwLock<HashMap<String, WasmHashValue>>>>,
    pub wasm_stream_info_map: ArcRwLock<HashMap<u64, Share<StreamInfo>>>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            wasm_plugin_map: ArcRwLock::new(HashMap::new()),

            wash_hash: WASH_HASH.clone(),
            wash_hash_hash: WASH_HASH_HASH.clone(),
            wasm_stream_info_map: WASM_STREAM_INFO_MAP.clone(),
        }
    }

    pub fn get_wasm_plugin(&self, path: &str) -> Result<Arc<WasmPlugin>> {
        let wasm_plugins = self.wasm_plugin_map.get().get(path).cloned();
        if wasm_plugins.is_some() {
            let wasm_plugins = wasm_plugins.unwrap();
            let wasm_plugin = wasm_plugins.get_mut().front().cloned();
            if wasm_plugin.is_some() {
                return Ok(wasm_plugin.unwrap());
            }
        }
        let mut config = Config::default();
        config.wasm_component_model(true);
        config.async_support(true);
        let engine = Engine::new(&config).map_err(|e| anyhow!("err:Engine => err:{}", e))?;
        let component = Component::from_file(&engine, path)
            .map_err(|e| anyhow!("err:Component::from_file => err:{}", e))?;

        let mut linker = Linker::new(&engine);

        WasmServer::add_to_linker(&mut linker, ServerWasiView::wasm_host)
            .map_err(|e| anyhow!("err:WasmServer::add_to_linker => err:{}", e))?;

        // Add the command world (aka WASI CLI) to the linker
        command::add_to_linker(&mut linker)
            .map_err(|e| anyhow!("err:command::sync::add_to_linker => err:{}", e))?;

        let wasm_plugin = Arc::new(WasmPlugin {
            path: path.to_string(),
            engine,
            component,
            linker,
        });
        self.push_wasm_plugin(wasm_plugin.clone())?;
        Ok(wasm_plugin)
    }

    pub fn pop_wasm_plugin(&self, path: &str) -> Result<Arc<WasmPlugin>> {
        let wasm_plugins = self.wasm_plugin_map.get().get(path).cloned();
        if wasm_plugins.is_some() {
            let wasm_plugins = wasm_plugins.unwrap();
            let wasm_plugin = wasm_plugins.get_mut().pop_front();
            if wasm_plugin.is_some() {
                return Ok(wasm_plugin.unwrap());
            }
        }
        let mut config = Config::default();
        config.wasm_component_model(true);
        config.async_support(true);
        let engine = Engine::new(&config).map_err(|e| anyhow!("err:Engine => err:{}", e))?;
        let component = Component::from_file(&engine, path)
            .map_err(|e| anyhow!("err:Component::from_file => err:{}", e))?;

        let mut linker = Linker::new(&engine);

        WasmServer::add_to_linker(&mut linker, ServerWasiView::wasm_host)
            .map_err(|e| anyhow!("err:WasmServer::add_to_linker => err:{}", e))?;

        // Add the command world (aka WASI CLI) to the linker
        command::add_to_linker(&mut linker)
            .map_err(|e| anyhow!("err:command::sync::add_to_linker => err:{}", e))?;

        Ok(Arc::new(WasmPlugin {
            path: path.to_string(),
            engine,
            component,
            linker,
        }))
    }

    pub fn push_wasm_plugin(&self, wasm_plugin: Arc<WasmPlugin>) -> Result<()> {
        let wasm_plugins = self.wasm_plugin_map.get().get(&wasm_plugin.path).cloned();
        let wasm_plugins = if wasm_plugins.is_none() {
            let wasm_plugins_ = ArcRwLock::new(VecDeque::with_capacity(100));
            let wasm_plugin_map = &mut *self.wasm_plugin_map.get_mut();
            let wasm_plugins = wasm_plugin_map.get(&wasm_plugin.path).cloned();
            if wasm_plugins.is_none() {
                wasm_plugin_map.insert(wasm_plugin.path.clone(), wasm_plugins_.clone());
                wasm_plugins_
            } else {
                wasm_plugins.unwrap()
            }
        } else {
            wasm_plugins.unwrap()
        };
        wasm_plugins.get_mut().push_back(wasm_plugin);
        Ok(())
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![]);
}

lazy_static! {
    pub static ref MODULE_FUNC: Arc<module::Func> = Arc::new(module::Func {
        create_conf: |ms| Box::pin(create_conf(ms)),
        merge_conf: |ms, parent_conf, child_conf| Box::pin(merge_conf(ms, parent_conf, child_conf)),

        init_conf: |ms, parent_conf, child_conf| Box::pin(init_conf(ms, parent_conf, child_conf)),
        merge_old_conf: |old_ms, old_main_conf, old_conf, ms, main_conf, conf| Box::pin(
            merge_old_conf(old_ms, old_main_conf, old_conf, ms, main_conf, conf)
        ),
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_core_wasm".to_string(),
        main_index: -1,
        ctx_index: -1,
        index: -1,
        ctx_index_len: -1,
        func: MODULE_FUNC.clone(),
        cmds: MODULE_CMDS.clone(),
        create_main_confs: None,
        init_main_confs: None,
        merge_old_main_confs: None,
        merge_confs: None,
        typ: conf::MODULE_TYPE_NET,
        create_server: None,
    });
}

pub fn module() -> typ::ArcRwLock<module::Module> {
    return M.clone();
}

pub async fn main_conf(ms: &module::Modules) -> &Conf {
    ms.get_main_conf::<Conf>(module()).await
}

pub async fn main_conf_mut(ms: &module::Modules) -> &mut Conf {
    ms.get_main_conf_mut::<Conf>(module()).await
}

pub async fn main_any_conf(ms: &module::Modules) -> ArcUnsafeAny {
    ms.get_main_any_conf(module()).await
}

pub fn curr_conf(curr: &ArcUnsafeAny) -> &Conf {
    module::Modules::get_curr_conf(curr, module())
}

pub fn curr_conf_mut(curr: &ArcUnsafeAny) -> &mut Conf {
    module::Modules::get_curr_conf_mut(curr, module())
}

pub fn currs_conf(curr: &Vec<ArcUnsafeAny>) -> &Conf {
    module::Modules::get_currs_conf(curr, module())
}

pub fn currs_conf_mut(curr: &Vec<ArcUnsafeAny>) -> &mut Conf {
    module::Modules::get_currs_conf_mut(curr, module())
}

pub fn curr_any_conf(curr: &ArcUnsafeAny) -> ArcUnsafeAny {
    module::Modules::get_curr_any_conf(curr, module())
}

pub fn currs_any_conf(curr: &Vec<ArcUnsafeAny>) -> ArcUnsafeAny {
    module::Modules::get_currs_any_conf(curr, module())
}

async fn create_conf(_ms: module::Modules) -> Result<typ::ArcUnsafeAny> {
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new())));
}

async fn merge_conf(
    _ms: module::Modules,
    _parent_conf: Option<typ::ArcUnsafeAny>,
    _child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}

async fn merge_old_conf(
    _old_ms: Option<module::Modules>,
    _old_main_conf: Option<typ::ArcUnsafeAny>,
    _old_conf: Option<typ::ArcUnsafeAny>,
    _ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}

async fn init_conf(
    _ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}
