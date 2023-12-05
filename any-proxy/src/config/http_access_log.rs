use crate::config as conf;
use crate::config::config_toml::default_access;
use crate::config::config_toml::AccessConfig;
use crate::proxy::access_log::AccessLog;
use crate::proxy::proxy::AccessContext;
use crate::proxy::stream_info::StreamInfo;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcUnsafeAny, Share};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessConfigs {
    pub access: Vec<AccessConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessConfigsDefault {
    #[serde(default = "default_access")]
    pub access: Vec<AccessConfig>,
}

pub struct Conf {
    pub is_default: bool,
    pub access: Arc<Vec<AccessConfig>>,
    pub access_context: Arc<Vec<AccessContext>>,
}

impl Conf {
    pub async fn new() -> Result<Self> {
        Ok(Conf {
            is_default: true,
            access: Arc::new(Vec::new()),
            access_context: Arc::new(Vec::new()),
        })
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "access".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(access(ms, conf_arg, cmd, conf)),
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
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "http_access_log".to_string(),
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
        typ: conf::MODULE_TYPE_HTTP,
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
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new().await?)));
}

async fn merge_conf(
    _ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    if parent_conf.is_none() {
        return Ok(());
    }
    let parent_conf = parent_conf.unwrap();
    let parent_conf = parent_conf.get_mut::<Conf>();
    let child_conf = child_conf.get_mut::<Conf>();
    if child_conf.is_default {
        child_conf.is_default = parent_conf.is_default;
        child_conf.access = parent_conf.access.clone();
        child_conf.access_context = parent_conf.access_context.clone();
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
    use crate::config::http_core_plugin;
    let http_core_plugin_conf = http_core_plugin::main_conf_mut(&ms).await;
    http_core_plugin_conf
        .plugin_handle_logs
        .get_mut()
        .await
        .push(|stream_info| Box::pin(access_log(stream_info)));
    return Ok(());
}

async fn access(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let access_conf: AccessConfigs =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!("access access_conf:{:?}", access_conf);
    let access = access_conf.access;

    if access.len() <= 0 {
        return Ok(());
    }

    let access_context = AccessLog::parse_config_access_log(&access).await?;
    conf.is_default = false;
    conf.access = Arc::new(access);
    conf.access_context = Arc::new(access_context);
    return Ok(());
}

pub async fn access_log(stream_info: Share<StreamInfo>) -> Result<()> {
    if !stream_info.get_mut().is_discard_flow {
        if stream_info.get_mut().scc.is_some() {
            AccessLog::access_log(stream_info)
                .await
                .map_err(|e| anyhow!("err:StreamStream::access_log => e:{}", e))?;
        }
    }
    Ok(())
}
