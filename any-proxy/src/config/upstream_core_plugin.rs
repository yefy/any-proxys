use crate::config as conf;
use crate::stream::connect;
use crate::upstream::UpstreamData;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;

pub type PluginHandleBalancer = fn(
    ip: &str,
    ups_data: &mut UpstreamData,
) -> Option<(Option<bool>, Arc<Box<dyn connect::Connect>>)>;

pub const WEIGHT: &str = "weight";
pub const ROUND_ROBIN: &str = "round_robin";
pub const RANDOM: &str = "random";
pub const IP_HASH: &str = "ip_hash";
pub const IP_HASH_ACTIVE: &str = "ip_hash_active";
pub const FAIR: &str = "fair";

pub struct Conf {
    pub plugin_handle_balancers: HashMap<String, PluginHandleBalancer>,
}

impl Conf {
    pub fn new() -> Self {
        let mut plugin_handle_balancers: HashMap<String, PluginHandleBalancer> = HashMap::new();
        use crate::upstream::balancer;
        plugin_handle_balancers.insert(WEIGHT.to_string(), balancer::weight::weight);
        plugin_handle_balancers.insert(ROUND_ROBIN.to_string(), balancer::round_robin::round_robin);
        plugin_handle_balancers.insert(RANDOM.to_string(), balancer::random::random);
        plugin_handle_balancers.insert(IP_HASH.to_string(), balancer::ip_hash::ip_hash);
        plugin_handle_balancers.insert(
            IP_HASH_ACTIVE.to_string(),
            balancer::ip_hash_active::ip_hash_active,
        );
        plugin_handle_balancers.insert(FAIR.to_string(), balancer::fair::fair);
        Conf {
            plugin_handle_balancers,
        }
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
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "upstream_core_plugin".to_string(),
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
        typ: conf::MODULE_TYPE_UPSTREAM,
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
