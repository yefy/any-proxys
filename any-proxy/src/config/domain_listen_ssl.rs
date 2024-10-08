use crate::config as conf;
use crate::config::config_toml::SslListenDomain;
use crate::config::net_core::{DomainFromHttpV1, TCP_CONFIG_NAME_DEFAULT};
use crate::proxy::domain_config::DomainConfigListenMerge;
use crate::proxy::MsConfigContext;
use crate::quic;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Conf {}

impl Conf {
    pub fn new() -> Self {
        Conf {}
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "domain_listen_ssl".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(domain_listen_ssl(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        //___wait___
        //conf_typ: conf::CMD_CONF_TYPE_MAIN | conf::CMD_CONF_TYPE_SERVER,
        conf_typ:  conf::CMD_CONF_TYPE_SERVER,
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
        name: "domain_listen_ssl".to_string(),
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

async fn domain_listen_ssl(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let ssl_listen: SslListenDomain =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "domain_listen_ssl ssl_listen:{:?}", ssl_listen);

    use crate::config::common_core;
    use crate::config::config_toml;
    use crate::config::config_toml::SSL;
    use crate::config::domain_core;
    use crate::config::net_core;
    use crate::config::net_server;
    use crate::config::net_server_core;
    use crate::proxy::domain_config::DomainConfigContext;
    use crate::util;
    let net_server_conf = net_server::curr_conf(conf_arg.curr_conf());
    let net_core_conf = net_core::curr_conf(conf_arg.curr_conf());
    let net_server_core_conf = net_server_core::curr_conf_mut(conf_arg.curr_conf());

    let common_core_any_conf = common_core::main_any_conf(&ms).await;
    let domain_core_conf = domain_core::main_conf_mut(&ms).await;

    if net_core_conf.domain.is_empty() {
        return Err(anyhow!("domain is nil"));
    }

    if net_server_core_conf.is_port_listen.is_some() {
        if net_server_core_conf.is_port_listen == Some(true) {
            return Err(anyhow!("err:not domain listen"));
        }
    } else {
        net_server_core_conf.is_port_listen = Some(false);
    }

    let scc = Arc::new(MsConfigContext::new(
        net_server_conf.net_confs.clone(),
        net_server_conf.server_confs.clone(),
        conf_arg.curr_conf().clone(),
        common_core_any_conf,
    ));

    let domain_config_context = Arc::new(DomainConfigContext { scc });

    let sock_addrs = util::util::str_to_socket_addrs(&ssl_listen.address)
        .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
    for addr in sock_addrs.iter() {
        let key = util::util::tcp_key_from_addr(addr)
            .map_err(|e| anyhow!("err:DomainConfig::tcp_key_from_addr => e:{}", e))?;
        if domain_core_conf
            .domain_config_listen_merge_map
            .get(&key)
            .is_none()
        {
            if domain_core_conf.key_map.get(&key).is_some() {
                return Err(anyhow!("err:key is exist => key:{}", key));
            }
            domain_core_conf.key_map.insert(key.clone(), true);
            domain_core_conf.domain_config_listen_merge_map.insert(
                key.clone(),
                ArcMutex::new(DomainConfigListenMerge {
                    ms: ms.clone(),
                    key: key.clone(),
                    listen_addr: None,
                    listens: Vec::new(),
                    domain_config_contexts: Vec::new(),
                    func: |data| Box::pin(parse_domain(data)),
                }),
            );
        }

        let values = domain_core_conf
            .domain_config_listen_merge_map
            .get_mut(&key)
            .unwrap();
        let mut values = values.get_mut();

        let ssl = SSL {
            ssl_domain: net_core_conf.domain.clone(),
            cert: ssl_listen.ssl.cert.clone(),
            key: ssl_listen.ssl.key.clone(),
            tls: ssl_listen.ssl.tls.clone(),
        };
        let listen = config_toml::Listen {
            address: addr.to_string(),
            ssl: Some(ssl),
        };

        values.listens.push(listen);
        values.listen_addr = Some(addr.clone());
        values
            .domain_config_contexts
            .push(domain_config_context.clone());
    }
    Ok(())
}

pub async fn parse_domain(value: ArcMutex<DomainConfigListenMerge>) -> Result<()> {
    use crate::config::domain_core;
    use crate::config::net_core;
    use crate::proxy::domain_config::DomainConfigListen;
    use crate::ssl::server as ssl_server;
    use crate::stream::server::Server;
    use crate::util;
    let key = value.get().key.clone();
    let ms = value.get().ms.clone();

    use crate::config::common_core;
    let common_core_conf = common_core::main_conf(&ms).await;
    use crate::config::socket_tcp;
    let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
    let domain_core_conf = domain_core::main_conf_mut(&ms).await;

    let mut domain_config_context_map = HashMap::new();
    let mut index_map = HashMap::new();
    let mut index = 0;
    let mut domain_from_http_v1 = Arc::new(DomainFromHttpV1::new());
    let mut tcp_config_name = TCP_CONFIG_NAME_DEFAULT.to_string();
    let mut plugin_handle_protocol = ArcRwLockTokio::default();
    let domain_config_contexts = value.get().domain_config_contexts.clone();
    for domain_config_context in domain_config_contexts.iter() {
        let scc = domain_config_context.scc.clone();
        let net_core_conf = net_core::currs_conf(scc.net_server_confs());

        if net_core_conf.domain_from_http_v1.is_open {
            if domain_from_http_v1.is_open {
                log::warn!("domain_from_http_v1 more open :{}", key);
            } else {
                domain_from_http_v1 = net_core_conf.domain_from_http_v1.clone();
            }
        }

        if &net_core_conf.tcp_config_name != TCP_CONFIG_NAME_DEFAULT {
            if &tcp_config_name != TCP_CONFIG_NAME_DEFAULT {
                log::warn!("more tcp_config_name :{}", key);
            } else {
                tcp_config_name = net_core_conf.tcp_config_name.clone();
            }
        }

        use crate::config::net_server_core_plugin;
        let net_server_core_plugin_conf =
            net_server_core_plugin::currs_conf(scc.net_server_confs());
        if net_server_core_plugin_conf
            .plugin_handle_protocol
            .is_some()
            .await
        {
            if plugin_handle_protocol.is_some().await {
                log::warn!("more plugin_handle_protocol :{}", key);
            } else {
                plugin_handle_protocol = net_server_core_plugin_conf.plugin_handle_protocol.clone();
            }
        }

        index += 1;
        index_map.insert(index, (net_core_conf.domain.clone(), index));
        domain_config_context_map.insert(index, domain_config_context.clone());

        let mut index_map_test = HashMap::new();
        index_map_test.insert(index, (net_core_conf.domain.clone(), index));
        util::domain_index::DomainIndex::new(&index_map_test)
            .map_err(|e| anyhow!("err:domain => domain:{:?}, e:{}", net_core_conf.domain, e))?;
    }
    let domain_index = Arc::new(
        util::domain_index::DomainIndex::new(&index_map)
            .map_err(|e| anyhow!("err:domain => index_map:{:?}, e:{}", index_map, e))?,
    );
    let value = value.get();
    let sni = quic::util::sni(&value.listens)?;

    let tcp_config = { upstream_tcp_conf.config(&tcp_config_name).unwrap() };
    let listen_server: Arc<Box<dyn Server>> = Arc::new(Box::new(ssl_server::Server::new(
        value.listen_addr.clone().unwrap(),
        common_core_conf.reuseport,
        tcp_config,
        sni.clone(),
    )?));

    domain_core_conf.domain_config_listen_map.insert(
        key.clone(),
        DomainConfigListen {
            listen_server,
            domain_config_context_map,
            domain_index,
            sni: Some(sni),
            plugin_handle_protocol,
            domain_from_http_v1,
        },
    );
    Ok(())
}
