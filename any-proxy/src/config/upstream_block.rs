use crate::config as conf;
use crate::config::config_toml::UpstreamDynamicDomain;
use crate::config::config_toml::UpstreamHeartbeat;
use crate::config::upstream_core;
use crate::config::upstream_core_plugin::PluginHandleBalancer;
use crate::upstream::{UpstreamDynamicDomainData, UpstreamHeartbeatData};
use crate::util;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcUnsafeAny, ShareRw};
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Conf {
    pub name: String,
    pub balancer: ArcString,
    pub plugin_handle_balancer: Option<PluginHandleBalancer>,
    pub ups_dynamic_domains: Vec<ShareRw<UpstreamDynamicDomainData>>,
    pub ups_heartbeats: Vec<ShareRw<UpstreamHeartbeatData>>,
    pub ups_heartbeats_active: Vec<ShareRw<UpstreamHeartbeatData>>,
    pub ups_heartbeats_index: usize,
    pub ups_heartbeats_map: HashMap<usize, ShareRw<UpstreamHeartbeatData>>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            name: "".to_string(),
            balancer: ArcString::default(),
            plugin_handle_balancer: None,
            ups_dynamic_domains: Vec::new(),
            ups_heartbeats: Vec::new(),
            ups_heartbeats_active: Vec::new(),
            ups_heartbeats_index: 0,
            ups_heartbeats_map: HashMap::new(),
        }
    }
    pub async fn add_proxy_pass(
        &mut self,
        ms: module::Modules,
        address: String,
        dynamic_domain: Option<UpstreamDynamicDomain>,
        heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>>,
    ) -> Result<()> {
        use crate::config::upstream_core_plugin;
        let ups_heartbeat = if self.balancer.as_str() == upstream_core_plugin::FAIR {
            use crate::config::config_toml::default_heartbeat_fail;
            use crate::config::config_toml::default_heartbeat_interval;
            use crate::config::config_toml::default_heartbeat_timeout;
            Some(UpstreamHeartbeat {
                interval: default_heartbeat_interval(),
                timeout: default_heartbeat_timeout(),
                fail: default_heartbeat_fail(),
                http: None,
            })
        } else {
            None
        };

        let is_weight = if self.balancer.as_str() == upstream_core_plugin::WEIGHT {
            true
        } else {
            false
        };

        let address = address;
        let dynamic_domain = dynamic_domain;
        let mut addrs = util::util::lookup_hosts(tokio::time::Duration::from_secs(30), &address)
            .await
            .map_err(|e| anyhow!("err:lookup_host => address:{} e:{}", address, e))?;
        addrs.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let domaon_index = self.ups_dynamic_domains.len();
        let ups_dynamic_domain = UpstreamDynamicDomainData {
            index: domaon_index,
            dynamic_domain,
            heartbeat: heartbeat.clone(),
            host: address.clone(),
            addrs: addrs.clone(),
            ups_heartbeat: ups_heartbeat.clone(),
            is_weight,
        };
        self.ups_dynamic_domains
            .push(ShareRw::new(ups_dynamic_domain));

        for addr in addrs.iter() {
            let ups_heartbeat = heartbeat
                .heartbeat(
                    ms.clone(),
                    domaon_index,
                    self.ups_heartbeats_index,
                    addr,
                    address.clone(),
                    ups_heartbeat.clone(),
                    is_weight,
                )
                .await?;
            self.ups_heartbeats.push(ups_heartbeat.clone());
            self.ups_heartbeats_active.push(ups_heartbeat.clone());
            self.ups_heartbeats_map
                .insert(self.ups_heartbeats_index, ups_heartbeat);
            self.ups_heartbeats_index += 1;
        }
        return Ok(());
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "balancer".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(balancer(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
    ]);
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
        name: "upstream_block".to_string(),
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
        let parent_conf = parent_conf.unwrap();
        let _parent_conf = parent_conf.get_mut::<Conf>();
    }
    let _child_conf = child_conf.get_mut::<Conf>();

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

async fn name(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.name = unsafe { conf_arg.value.take::<String>() };
    log::trace!(target: "main", "c.name:{:?}", c.name);
    return Ok(());
}

async fn balancer(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.balancer = ArcString::new(conf_arg.value.get_mut::<String>().clone());
    log::trace!(target: "main", "c.dispatch:{:?}", c.balancer);
    use crate::config::upstream_core_plugin;
    let upstream_core_plugin_conf = upstream_core_plugin::main_conf_mut(&ms).await;

    let plugin_handle_balancer = upstream_core_plugin_conf
        .plugin_handle_balancers
        .get(c.balancer.as_str())
        .cloned();
    if plugin_handle_balancer.is_none() {
        return Err(anyhow!("balancer:{} invalid", c.balancer));
    }
    c.plugin_handle_balancer = Some(plugin_handle_balancer.unwrap());
    return Ok(());
}
