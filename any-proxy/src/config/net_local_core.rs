use crate::config as conf;
use crate::proxy::stream_var;
use crate::util::var::Var;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcUnsafeAny, OptionExt};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub data: String,
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    rule: Vec<Rule>,
}

pub struct LocalRule {
    pub rule: Rule,
    pub data_format_vars: Var,
    pub regex: OptionExt<Regex>,
    pub local: ArcUnsafeAny,
}

pub struct Conf {
    pub local_rules: Vec<Arc<LocalRule>>,
    pub full_match_local_rule: OptionExt<Arc<LocalRule>>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            local_rules: Vec::new(),
            full_match_local_rule: None.into(),
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "rule".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(rule(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_LOCAL,
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
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_local_core".to_string(),
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
    ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    use crate::config::net;
    let net_conf = net::main_conf(&ms).await;
    for net_server_conf in &net_conf.sub_confs {
        use crate::config::net_server;
        let net_server_conf = net_server_conf.get::<net_server::Conf>();

        use crate::config::net_local_core;
        let net_local_core_conf = net_local_core::currs_conf(&net_server_conf.server_confs);
        if net_local_core_conf.full_match_local_rule.is_none() {
            return Err(anyhow::anyhow!(
                "net_local_core_conf.full_match_local_rule.is_none"
            ));
        }
        // for local_rule in &net_local_core_conf.local_rules {
        //     log::info!("rule:{:?}", local_rule.rule);
        // }

        // for net_local_conf in &net_server_conf.sub_confs {
        //     use crate::config::net_local;
        //     let net_local_conf = net_local_conf.get::<net_local::Conf>();
        //
        //     use crate::config::net_core;
        //     use crate::config::net_server_core;
        //     let net_server_core_conf = net_server_core::currs_conf(&net_local_conf.local_confs);
        //     let net_core_conf = net_core::currs_conf(&net_local_conf.server_confs);
        //     if net_server_core_conf.upstream_data.is_none() {
        //         return Err(anyhow::anyhow!(
        //             "net_proxy_pass is_none, domain:{}",
        //             net_core_conf.domain
        //         ));
        //     }
        // }
    }
    return Ok(());
}

async fn rule(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let rules: Rules = toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "c.rules:{:?}", rules);
    use crate::config::net_local;
    use crate::config::net_local_core;
    let net_local_conf = net_local::curr_conf(conf_arg.curr_conf());
    let net_local_core_conf = net_local_core::currs_conf_mut(&net_local_conf.server_confs);
    let local_any = conf_arg.curr_conf().clone();
    use crate::util::default_config::VAR_STREAM_INFO;

    for rule in rules.rule {
        let ret: Result<Var> = async {
            let data_format_vars =
                Var::new(&rule.data, "-").map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
            let mut data_format_vars_test =
                Var::copy(&data_format_vars).map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            data_format_vars_test.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &VAR_STREAM_INFO)
                    .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                Ok(value)
            })?;
            let _ = data_format_vars_test
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            Ok(data_format_vars)
        }
        .await;
        let data_format_vars =
            ret.map_err(|e| anyhow!("err:rule.data => rule.data:{}, e:{}", rule.data, e))?;

        {
            let v = rule.filter.trim();
            if v.len() <= 0 {
                return Err(anyhow::anyhow!("err:rule.filter => {}", rule.filter));
            }
            if v.len() >= 3 && &v[0..2] == "$$" {
                let v = &v[2..];
                log::trace!(target: "main", "v:{}", v);
                if v == "(.*)" {
                    if net_local_core_conf.full_match_local_rule.is_some() {
                        return Err(anyhow!(
                            "err:full_match_local_rule exist => full_match_local_rule:{}",
                            v
                        ));
                    }
                    net_local_core_conf.full_match_local_rule = Some(Arc::new(LocalRule {
                        rule: rule.clone(),
                        data_format_vars,
                        regex: None.into(),
                        local: local_any.clone(),
                    }))
                    .into();
                    continue;
                }
                let regex = Regex::new(v).map_err(|e| anyhow!("err:Regex::new => e:{}", e))?;
                net_local_core_conf.local_rules.push(Arc::new(LocalRule {
                    rule: rule.clone(),
                    data_format_vars,
                    regex: Some(regex).into(),
                    local: local_any.clone(),
                }));
            } else {
                let local_rule = Arc::new(LocalRule {
                    rule: rule.clone(),
                    data_format_vars,
                    regex: None.into(),
                    local: local_any.clone(),
                });
                net_local_core_conf.local_rules.push(local_rule);
            }
        }
    }

    return Ok(());
}
