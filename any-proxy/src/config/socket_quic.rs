use crate::config as conf;
use crate::config::config_toml;
use crate::config::config_toml::default_quic_config;
use crate::config::config_toml::default_quic_config_name;
use crate::config::config_toml::QuicConfig;
use crate::quic::endpoints;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuicConfigs {
    #[serde(default = "default_quic_config")]
    #[serde(rename = "quic")]
    confs: Vec<QuicConfig>,
}

pub struct Conf {
    pub quic_confs: HashMap<String, Arc<QuicConfig>>,
    pub endpoints_map: HashMap<String, Arc<endpoints::Endpoints>>,
}

impl Conf {
    pub fn new(_ms: module::Modules) -> Result<Self> {
        let mut conf = Conf {
            quic_confs: HashMap::new(),
            endpoints_map: HashMap::new(),
        };

        let quic_confs_default: QuicConfigs =
            toml::from_str("").map_err(|e| anyhow!("err: => e:{}", e))?;

        for quic_conf in &quic_confs_default.confs {
            if conf.quic_confs.get(&quic_conf.quic_config_name).is_some() {
                continue;
            }
            conf.quic_confs.insert(
                quic_conf.quic_config_name.clone(),
                Arc::new(quic_conf.clone()),
            );
            conf.endpoints_map.insert(
                quic_conf.quic_config_name.clone(),
                Arc::new(endpoints::Endpoints::new(quic_conf, false)?),
            );
        }
        Ok(conf)
    }

    pub fn config(&self, str: &str) -> Option<Arc<config_toml::QuicConfig>> {
        self.quic_confs.get(str).cloned()
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "quic".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(quic(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_MAIN | conf::CMD_CONF_TYPE_SERVER | conf::CMD_CONF_TYPE_LOCAL,
    }]);
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
        name: "socket_quic".to_string(),
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
        typ: conf::MODULE_TYPE_SOCKET,
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

async fn create_conf(ms: module::Modules) -> Result<typ::ArcUnsafeAny> {
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new(ms)?)));
}

async fn merge_conf(
    _ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    if parent_conf.is_some() {
        let mut _parent_conf = parent_conf.unwrap().get_mut::<Conf>();
    }
    let mut _child_conf = child_conf.get_mut::<Conf>();
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
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    return Ok(());
}

async fn quic(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let quic_confs: QuicConfigs =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "socket_quic quic_confs:{:?}", quic_confs);

    for quic_conf in &quic_confs.confs {
        if quic_conf.quic_config_name != default_quic_config_name() {
            if conf.quic_confs.get(&quic_conf.quic_config_name).is_some() {
                return Err(anyhow!("err:{:?}", quic_conf.quic_config_name));
            }
        }
        conf.quic_confs.insert(
            quic_conf.quic_config_name.clone(),
            Arc::new(quic_conf.clone()),
        );
        conf.endpoints_map.insert(
            quic_conf.quic_config_name.clone(),
            Arc::new(endpoints::Endpoints::new(quic_conf, false)?),
        );
    }
    return Ok(());
}
