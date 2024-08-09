use crate::config as conf;
use crate::config::config_toml::SslListenPort;
use crate::proxy::MsConfigContext;
use crate::ssl::server as ssl_server;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
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
        name: "port_listen_ssl".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(port_listen_ssl(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_SERVER,
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
        name: "port_listen_ssl".to_string(),
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

async fn port_listen_ssl(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let ssl_listen: SslListenPort =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "port_listen_ssl ssl_listen:{:?}", ssl_listen);

    use crate::config::common_core;
    use crate::config::config_toml;
    use crate::config::net_core;
    use crate::config::net_server;
    use crate::config::net_server_core;
    use crate::config::port_core;
    use crate::config::socket_tcp;
    use crate::proxy::port_config::PortConfigContext;
    use crate::proxy::port_config::PortConfigListen;
    use crate::quic;
    use crate::stream::server;
    use crate::util;
    let net_server_conf = net_server::curr_conf(conf_arg.curr_conf());
    let net_core_conf = net_core::curr_conf(conf_arg.curr_conf());
    let net_server_core_conf = net_server_core::curr_conf_mut(conf_arg.curr_conf());

    let common_core_conf = common_core::main_conf(&ms).await;
    let common_core_any_conf = common_core::main_any_conf(&ms).await;
    let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
    let port_core_conf = port_core::main_conf_mut(&ms).await;

    if net_server_core_conf.is_port_listen.is_some() {
        if net_server_core_conf.is_port_listen == Some(false) {
            return Err(anyhow!("err:not port listen"));
        }
    } else {
        net_server_core_conf.is_port_listen = Some(true);
    }

    let scc = Arc::new(MsConfigContext::new(
        net_server_conf.net_confs.clone(),
        net_server_conf.server_confs.clone(),
        conf_arg.curr_conf().clone(),
        common_core_any_conf,
    ));

    let port_config_context = Arc::new(PortConfigContext { scc });

    let sock_addrs = util::util::str_to_socket_addrs(&ssl_listen.address)
        .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
    for addr in sock_addrs.iter() {
        let key = util::util::tcp_key_from_addr(addr)
            .map_err(|e| anyhow!("err:PortConfig::tcp_key_from_addr => e:{}", e))?;
        if port_core_conf.port_config_listen_map.get(&key).is_some() {
            return Err(anyhow!("err:addr is exist => addr:{}", addr));
        }

        let tcp_config = upstream_tcp_conf.config(&net_core_conf.tcp_config_name);
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name nil => tcp_config_name:{}",
                net_core_conf.tcp_config_name
            ));
        }

        let listen = config_toml::Listen {
            address: addr.to_string(),
            ssl: Some(ssl_listen.ssl.clone()),
        };

        let sni = quic::util::sni(&vec![listen.clone()])
            .map_err(|e| anyhow!("err:util::sni => e:{}", e))?;

        let listen_server: Arc<Box<dyn server::Server>> =
            Arc::new(Box::new(ssl_server::Server::new(
                addr.clone(),
                common_core_conf.reuseport,
                tcp_config.unwrap(),
                sni,
            )?));
        port_core_conf.port_config_listen_map.insert(
            key,
            PortConfigListen {
                listen_server,
                port_config_context: port_config_context.clone(),
            },
        );
    }
    Ok(())
}
