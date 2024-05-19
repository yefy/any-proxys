use crate::config as conf;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use tokio::sync::OnceCell;
static ONCE: OnceCell<Result<()>> = OnceCell::const_new();

#[cfg(feature = "anyproxy-ebpf")]
lazy_static! {
    pub static ref EBPF_GROUP: typ::ArcMutex<any_ebpf::EbpfGroup> = typ::ArcMutex::default();
}

#[cfg(feature = "anyproxy-ebpf")]
lazy_static! {
    pub static ref EBPF_TX: typ::ArcMutex<any_ebpf::AnyEbpfTx> = typ::ArcMutex::default();
}

fn default_debug_is_open_ebpf_log() -> bool {
    false
}
fn default_is_open_ebpf() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EbpfConf {
    #[serde(default = "default_debug_is_open_ebpf_log")]
    pub debug_is_open_ebpf_log: bool,
    #[serde(default = "default_is_open_ebpf")]
    pub is_open_ebpf: bool,
}

impl EbpfConf {
    pub fn new() -> Self {
        EbpfConf {
            debug_is_open_ebpf_log: default_debug_is_open_ebpf_log(),
            is_open_ebpf: default_is_open_ebpf(),
        }
    }
}

pub struct Conf {
    ebpf_conf: EbpfConf,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_tx: Option<any_ebpf::AnyEbpfTx>,
}

impl Drop for Conf {
    fn drop(&mut self) {
        log::debug!(target: "ms", "drop any_ebpf_core");
    }
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            ebpf_conf: EbpfConf::new(),
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_tx: None,
        }
    }
    #[cfg(feature = "anyproxy-ebpf")]
    pub fn ebpf(&self) -> Option<any_ebpf::AnyEbpfTx> {
        if !self.ebpf_tx.is_none() {
            self.ebpf_tx.clone()
        } else {
            None
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "data".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(data(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_MAIN,
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
        name: "any_ebpf_core".to_string(),
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
        typ: conf::MODULE_TYPE_EBPF,
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
    #[cfg(feature = "anyproxy-ebpf")]
    let conf = _conf.get_mut::<Conf>();
    #[cfg(feature = "anyproxy-ebpf")]
    if conf.ebpf_conf.is_open_ebpf && EBPF_TX.is_some() {
        conf.ebpf_tx = Some(EBPF_TX.get().clone());
    }
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

async fn data(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let ebpf_conf: EbpfConf =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ebpf_conf:{:?}", ebpf_conf);
    conf.ebpf_conf = ebpf_conf.clone();

    ONCE.get_or_init(|| async move {
        #[cfg(feature = "anyproxy-ebpf")]
        let mut ebpf_group = any_ebpf::EbpfGroup::new(false, 0);
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_tx = ebpf_group.start(ebpf_conf.debug_is_open_ebpf_log).await?;
        #[cfg(feature = "anyproxy-ebpf")]
        EBPF_GROUP.set(ebpf_group);
        #[cfg(feature = "anyproxy-ebpf")]
        EBPF_TX.set(ebpf_tx);
        Ok(())
    })
    .await;

    return Ok(());
}
