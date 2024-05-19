use crate::proxy::http_proxy::http_filter::http_filter_headers_pre::{
    http_filter_headers_pre, set_header_filter,
};
use crate::{config as conf, WasmPluginConfs};
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::path::Path;
use std::sync::Arc;

pub struct Conf {
    pub wasm_plugin_confs: Option<WasmPluginConfs>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            wasm_plugin_confs: None,
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "wasm_http_filter_headers_pre".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(wasm_http_filter_headers_pre(
            ms, conf_arg, cmd, conf
        )),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_MAIN | conf::CMD_CONF_TYPE_SERVER | conf::CMD_CONF_TYPE_LOCAL,
    },]);
}

lazy_static! {
    pub static ref MODULE_FUNC: Arc<module::Func> = Arc::new(module::Func {
        create_conf: |ms| Box::pin(create_conf(ms)),
        merge_conf: |ms, parent_conf, child_conf| Box::pin(merge_conf(ms, parent_conf, child_conf)),

        init_conf: |ms, parent_conf, child_conf| Box::pin(init_conf(ms, parent_conf, child_conf)),
        merge_old_conf: |old_ms, old_main_conf, old_conf, ms, main_conf, conf| Box::pin(
            merge_old_conf(old_ms, old_main_conf, old_conf, ms, main_conf, conf)
        ),
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "http_filter_headers_pre".to_string(),
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
        init_master_thread_confs: None,
        init_work_thread_confs: None,
        drop_confs: None,
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
    ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let child_conf = child_conf.get_mut::<Conf>();
    if parent_conf.is_some() {
        let parent_conf = parent_conf.unwrap();
        let parent_conf = parent_conf.get_mut::<Conf>();

        if child_conf.wasm_plugin_confs.is_none() {
            child_conf.wasm_plugin_confs = parent_conf.wasm_plugin_confs.clone();
        }
    }

    if child_conf.wasm_plugin_confs.is_some() {
        let wasm_plugin_confs = child_conf.wasm_plugin_confs.as_ref().unwrap();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        for wasm_plugin_conf in &wasm_plugin_confs.wasm {
            let _ = net_core_wasm_conf.get_wasm_plugin(&wasm_plugin_conf.wasm_path)?;
        }
    }

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
    ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    use crate::config::net_core_plugin;
    let plugin_http_header_filter_set = {
        let net_core_plugin_conf = net_core_plugin::main_conf_mut(&ms).await;
        net_core_plugin_conf.plugin_http_header_filter_set.clone()
    };

    {
        let plugin_http_header_filter_set = plugin_http_header_filter_set.get_mut().await;
        (plugin_http_header_filter_set)(|r| Box::pin(http_filter_headers_pre(r))).await?;
    }

    plugin_http_header_filter_set
        .set(|plugin| Box::pin(set_header_filter(plugin)))
        .await;

    return Ok(());
}

async fn wasm_http_filter_headers_pre(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let wasm_plugin_confs: WasmPluginConfs =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    for wasm_plugin_conf in &wasm_plugin_confs.wasm {
        if !Path::new(&wasm_plugin_conf.wasm_path).exists() {
            return Err(anyhow::anyhow!(
                "err:path => wasm_path:{}",
                wasm_plugin_conf.wasm_path
            ));
        }
    }

    c.wasm_plugin_confs = Some(wasm_plugin_confs);
    log::trace!(target: "main", "c.wasm_plugin_confs:{:?}", c.wasm_plugin_confs);
    return Ok(());
}
