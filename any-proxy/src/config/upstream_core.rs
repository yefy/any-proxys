use crate::config as conf;
use crate::config::upstream_block;
use crate::upstream::{UpstreamData, UpstreamHeartbeatData};
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use any_base::typ::{ArcMutex, Share};
use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use module::Modules;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[async_trait]
pub trait HeartbeatI: Send + Sync + 'static {
    async fn heartbeat(
        &self,
        ms: Modules,
        domain_index: usize,
        ups_heartbeats_index: usize,
        addr: &SocketAddr,
        host: String,
        ups_heartbeat: Option<UpstreamHeartbeat>,
        is_weight: bool,
    ) -> Result<Share<UpstreamHeartbeatData>>;
}

pub struct Conf {
    pub servers: Vec<Arc<upstream_block::Conf>>,
    pub ups_data_map: HashMap<String, ArcMutex<UpstreamData>>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            servers: Vec::new(),
            ups_data_map: HashMap::new(),
        }
    }

    pub fn add_upstream(&mut self, upstream_block: upstream_block::Conf) -> Result<()> {
        let upstream_block = Arc::new(upstream_block);
        let ups_data = ArcMutex::new(UpstreamData {
            is_heartbeat_disable: false,
            is_dynamic_domain_change: false,
            is_sort_heartbeats_active: false,
            ups_dynamic_domains: upstream_block.ups_dynamic_domains.clone(),
            ups_heartbeats: upstream_block.ups_heartbeats.clone(),
            ups_heartbeats_active: upstream_block.ups_heartbeats_active.clone(),
            ups_heartbeats_map: upstream_block.ups_heartbeats_map.clone(),
            ups_heartbeats_index: upstream_block.ups_heartbeats_index.clone(),
            ups_config: upstream_block.clone(),
            round_robin_index: 0,
        });
        self.ups_data_map
            .insert(upstream_block.name.clone(), ups_data);
        self.servers.push(upstream_block);

        Ok(())
    }

    pub fn upstream_data(&self, name: &str) -> Option<ArcMutex<UpstreamData>> {
        self.ups_data_map.get(name).cloned()
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "server".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(server(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_BLOCK,
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
        name: "upstream_core".to_string(),
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
        typ: conf::MODULE_TYPE_UPSTREAM,
        create_server: Some(create_server),
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
use crate::config::config_toml::UpstreamHeartbeat;
use crate::upstream::upstream::Upstream;
use std::collections::HashMap;
use std::net::SocketAddr;

fn create_server(value: typ::ArcUnsafeAny) -> Result<Box<dyn module::Server>> {
    let upstream = Upstream::new(value)?;
    Ok(Box::new(upstream))
}

async fn server(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let module_conf = typ::ArcUnsafeAny::new(Box::new(upstream_block::Conf::new()));
    let mut conf_arg_sub = module::ConfArg::new();
    conf_arg_sub
        .module_type
        .store(conf::MODULE_TYPE_UPSTREAM, Ordering::SeqCst);
    conf_arg_sub
        .cmd_conf_type
        .store(conf::CMD_CONF_TYPE_MAIN, Ordering::SeqCst);
    conf_arg_sub
        .main_index
        .store(conf_arg.main_index.load(Ordering::SeqCst), Ordering::SeqCst);
    conf_arg_sub.reader = conf_arg.reader.clone();
    conf_arg_sub.file_name = conf_arg.file_name.clone();
    conf_arg_sub.path = conf_arg.path.clone();
    conf_arg_sub.line_num = conf_arg.line_num.clone();
    conf_arg_sub.is_block = true;

    ms.parse_config(&mut conf_arg_sub, module_conf.clone())
        .await?;

    let module_conf = unsafe { module_conf.take::<upstream_block::Conf>() };
    let conf = conf.get_mut::<Conf>();
    conf.add_upstream(module_conf)
}
