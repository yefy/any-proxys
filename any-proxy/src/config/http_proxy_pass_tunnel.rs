use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;

use crate::config::config_toml::ProxyPassQuicTunnel;
use crate::config::config_toml::ProxyPassSslTunnel;
use crate::config::config_toml::ProxyPassTcpTunnel;
use any_base::typ::ArcUnsafeAny;

pub struct Conf {}

impl Conf {
    pub fn new() -> Self {
        Conf {}
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "proxy_pass_tunnel_tcp".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel_tcp(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER
        },
        module::Cmd {
            name: "proxy_pass_tunnel_ssl".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel_ssl(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER
        },
        module::Cmd {
            name: "proxy_pass_tunnel_quic".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel_quic(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER
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
        name: "http_proxy_pass_tunnel".to_string(),
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

async fn proxy_pass_tunnel_tcp(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassTcpTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!("ProxyPassTcpTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    use crate::config::upstream_block;
    use crate::config::upstream_core;
    use crate::config::upstream_core_plugin;
    let mut upstream_block = upstream_block::Conf::new();
    upstream_block.name = format!("tcp_tunnel_{:?}", proxy_pass_conf);
    upstream_block.balancer = upstream_core_plugin::ROUND_ROBIN.to_string();

    let upstream_core_plugin_conf = upstream_core_plugin::main_conf_mut(&ms).await;

    let plugin_handle_balancer = upstream_core_plugin_conf
        .plugin_handle_balancers
        .get(&upstream_block.balancer)
        .cloned();
    if plugin_handle_balancer.is_none() {
        return Err(anyhow!("balancer:{} invalid", upstream_block.balancer));
    }
    upstream_block.plugin_handle_balancer = Some(plugin_handle_balancer.unwrap());

    use crate::config::http_server_core;
    let http_server_core_conf = http_server_core::curr_conf_mut(conf_arg.curr_conf());
    if http_server_core_conf.upstream_name.len() > 0 {
        return Err(anyhow!("err:str:{}", str));
    }
    http_server_core_conf.upstream_name = upstream_block.name.clone();

    use super::upstream_proxy_pass_tunnel;
    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> = Arc::new(Box::new(
        upstream_proxy_pass_tunnel::HeartbeatTcp::new(proxy_pass_conf.clone()),
    ));

    upstream_block
        .add_proxy_pass(
            ms.clone(),
            proxy_pass_conf.address.clone(),
            proxy_pass_conf.dynamic_domain.clone(),
            heartbeat,
        )
        .await?;

    let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
    upstream_core_conf.add_upstream(upstream_block)?;
    let upstream_data = upstream_core_conf.upstream_data(&http_server_core_conf.upstream_name);
    if upstream_data.is_none() {
        return Err(anyhow!(
            "err:upstream_name  {} nil",
            http_server_core_conf.upstream_name
        ));
    }
    http_server_core_conf.upstream_data = upstream_data.unwrap();
    Ok(())
}

async fn proxy_pass_tunnel_ssl(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassSslTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!("ProxyPassSslTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    use crate::config::upstream_block;
    use crate::config::upstream_core;
    use crate::config::upstream_core_plugin;
    let mut upstream_block = upstream_block::Conf::new();
    upstream_block.name = format!("ssl_tunnel_{:?}", proxy_pass_conf);
    upstream_block.balancer = upstream_core_plugin::ROUND_ROBIN.to_string();

    let upstream_core_plugin_conf = upstream_core_plugin::main_conf_mut(&ms).await;

    let plugin_handle_balancer = upstream_core_plugin_conf
        .plugin_handle_balancers
        .get(&upstream_block.balancer)
        .cloned();
    if plugin_handle_balancer.is_none() {
        return Err(anyhow!("balancer:{} invalid", upstream_block.balancer));
    }
    upstream_block.plugin_handle_balancer = Some(plugin_handle_balancer.unwrap());

    use crate::config::http_server_core;
    let http_server_core_conf = http_server_core::curr_conf_mut(conf_arg.curr_conf());
    if http_server_core_conf.upstream_name.len() > 0 {
        return Err(anyhow!("err:str:{}", str));
    }
    http_server_core_conf.upstream_name = upstream_block.name.clone();

    use super::upstream_proxy_pass_tunnel;
    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> = Arc::new(Box::new(
        upstream_proxy_pass_tunnel::HeartbeatSsl::new(proxy_pass_conf.clone()),
    ));

    upstream_block
        .add_proxy_pass(
            ms.clone(),
            proxy_pass_conf.address.clone(),
            proxy_pass_conf.dynamic_domain.clone(),
            heartbeat,
        )
        .await?;

    let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
    upstream_core_conf.add_upstream(upstream_block)?;
    let upstream_data = upstream_core_conf.upstream_data(&http_server_core_conf.upstream_name);
    if upstream_data.is_none() {
        return Err(anyhow!(
            "err:upstream_name  {} nil",
            http_server_core_conf.upstream_name
        ));
    }
    http_server_core_conf.upstream_data = upstream_data.unwrap();
    Ok(())
}

async fn proxy_pass_tunnel_quic(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassQuicTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!("ProxyPassQuicTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    use crate::config::upstream_block;
    use crate::config::upstream_core;
    use crate::config::upstream_core_plugin;
    let mut upstream_block = upstream_block::Conf::new();
    upstream_block.name = format!("quic_tunnel_{:?}", proxy_pass_conf);
    upstream_block.balancer = upstream_core_plugin::ROUND_ROBIN.to_string();

    let upstream_core_plugin_conf = upstream_core_plugin::main_conf_mut(&ms).await;

    let plugin_handle_balancer = upstream_core_plugin_conf
        .plugin_handle_balancers
        .get(&upstream_block.balancer)
        .cloned();
    if plugin_handle_balancer.is_none() {
        return Err(anyhow!("balancer:{} invalid", upstream_block.balancer));
    }
    upstream_block.plugin_handle_balancer = Some(plugin_handle_balancer.unwrap());

    use crate::config::http_server_core;
    let http_server_core_conf = http_server_core::curr_conf_mut(conf_arg.curr_conf());
    if http_server_core_conf.upstream_name.len() > 0 {
        return Err(anyhow!("err:str:{}", str));
    }
    http_server_core_conf.upstream_name = upstream_block.name.clone();

    use super::upstream_proxy_pass_tunnel;
    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> = Arc::new(Box::new(
        upstream_proxy_pass_tunnel::HeartbeatQuic::new(proxy_pass_conf.clone()),
    ));

    upstream_block
        .add_proxy_pass(
            ms.clone(),
            proxy_pass_conf.address.clone(),
            proxy_pass_conf.dynamic_domain.clone(),
            heartbeat,
        )
        .await?;

    let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
    upstream_core_conf.add_upstream(upstream_block)?;
    let upstream_data = upstream_core_conf.upstream_data(&http_server_core_conf.upstream_name);
    if upstream_data.is_none() {
        return Err(anyhow!(
            "err:upstream_name  {} nil",
            http_server_core_conf.upstream_name
        ));
    }
    http_server_core_conf.upstream_data = upstream_data.unwrap();
    Ok(())
}
