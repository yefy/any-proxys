use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcRwLockTokio;
use any_base::typ::ArcUnsafeAny;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;

pub struct Conf {}

impl Conf {
    pub fn new() -> Self {
        Conf {}
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "net_server_proxy_websocket".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(net_server_proxy_websocket(
            ms, conf_arg, cmd, conf
        )),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_SERVER
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
        name: "net_server_proxy_websocket".to_string(),
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

async fn net_server_proxy_websocket(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    log::trace!(target: "main", "net_server_proxy_websocket str:{:?}", str);

    use crate::config::net_server_core_plugin;
    let net_server_core_plugin_conf = net_server_core_plugin::curr_conf_mut(conf_arg.curr_conf());

    use crate::proxy::websocket_proxy::websocket_server;
    net_server_core_plugin_conf.plugin_handle_protocol =
        ArcRwLockTokio::new(|arg, flow| Box::pin(websocket_server::http_server_handle(arg, flow)));

    return Ok(());
}
