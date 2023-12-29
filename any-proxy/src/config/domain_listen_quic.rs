use crate::config as conf;
use crate::config::config_toml::QuicListenDomain;
use crate::proxy::domain_config::DomainConfigListenMerge;
use crate::quic;
use crate::quic::server as quic_server;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcMutex, ArcUnsafeAny, ShareRw};
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
        name: "domain_listen_quic".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(domain_listen_quic(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_MAIN | conf::CMD_CONF_TYPE_SERVER,
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
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "domain_listen_quic".to_string(),
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

async fn domain_listen_quic(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let quic_listen: QuicListenDomain =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!("domain_listen_quic quic_listen:{:?}", quic_listen);

    use crate::config::common_core;
    use crate::config::config_toml;
    use crate::config::config_toml::SSL;
    use crate::config::domain_core;
    use crate::config::http_core;
    use crate::config::http_server_core;
    use crate::config::httpserver;
    use crate::proxy::domain_config::DomainConfigContext;
    use crate::proxy::StreamConfigContext;
    use crate::util;
    let httpserver_conf = httpserver::curr_conf(conf_arg.curr_conf());
    let http_core_conf = http_core::curr_conf(conf_arg.curr_conf());
    let http_server_core_conf = http_server_core::curr_conf_mut(conf_arg.curr_conf());

    let common_core_any_conf = common_core::main_any_conf(&ms).await;
    let domain_core_conf = domain_core::main_conf_mut(&ms).await;

    if http_core_conf.domain.is_empty() {
        return Err(anyhow!("domain is nil"));
    }

    if http_server_core_conf.is_port_listen.is_some() {
        if http_server_core_conf.is_port_listen == Some(true) {
            return Err(anyhow!("err:not domain listen"));
        }
    } else {
        http_server_core_conf.is_port_listen = Some(false);
    }

    let http_confs = httpserver_conf.http_confs.clone();
    let server_confs = httpserver_conf.server_confs.clone();
    let scc = ShareRw::new(StreamConfigContext::new(
        ms.clone(),
        http_confs,
        server_confs,
        conf_arg.curr_conf().clone(),
        common_core_any_conf,
    ));

    let domain_config_context = Arc::new(DomainConfigContext { scc: scc.clone() });

    let sock_addrs = util::util::str_to_socket_addrs(&quic_listen.address)
        .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
    for addr in sock_addrs.iter() {
        let udp_key = util::util::udp_key_from_addr(addr)
            .map_err(|e| anyhow!("err:DomainConfig::udp_key_from_addr => e:{}", e))?;
        let quic_key = util::util::quic_key_from_addr(addr)
            .map_err(|e| anyhow!("err:DomainConfig::quic_key_from_addr => e:{}", e))?;
        if domain_core_conf
            .domain_config_listen_merge_map
            .get(&quic_key)
            .is_none()
        {
            if domain_core_conf.key_map.get(&udp_key).is_some() {
                return Err(anyhow!("err:udp_key is exist => key:{}", udp_key));
            }
            domain_core_conf.key_map.insert(udp_key.clone(), true);
            domain_core_conf.domain_config_listen_merge_map.insert(
                quic_key.clone(),
                ArcMutex::new(DomainConfigListenMerge {
                    ms: ms.clone(),
                    key: quic_key.clone(),
                    listen_addr: None,
                    listens: Vec::new(),
                    domain_config_contexts: Vec::new(),
                    func: |data| Box::pin(parse_domain(data)),
                }),
            );
        }

        let values = domain_core_conf
            .domain_config_listen_merge_map
            .get_mut(&quic_key)
            .unwrap();
        let mut values = values.get_mut();

        let ssl = SSL {
            ssl_domain: http_core_conf.domain.clone(),
            cert: quic_listen.ssl.cert.clone(),
            key: quic_listen.ssl.key.clone(),
            tls: quic_listen.ssl.tls.clone(),
        };
        let listen = config_toml::Listen {
            address: addr.to_string(),
            ssl: Some(ssl),
        };

        values.listen_addr = Some(addr.clone());
        values.listens.push(listen);
        values
            .domain_config_contexts
            .push(domain_config_context.clone());
    }
    Ok(())
}

pub async fn parse_domain(value: ArcMutex<DomainConfigListenMerge>) -> Result<()> {
    use crate::config::domain_core;
    use crate::config::http_core;
    use crate::proxy::domain_config::DomainConfigListen;
    use crate::stream::server::Server;
    use crate::util;
    let key = value.get().key.clone();
    let ms = value.get().ms.clone();
    use crate::config::common_core;
    let common_core_conf = common_core::main_conf(&ms).await;
    #[cfg(feature = "anyproxy-ebpf")]
    use crate::config::any_ebpf_core;
    #[cfg(feature = "anyproxy-ebpf")]
    let any_ebpf_core_conf = any_ebpf_core::main_conf(&ms).await;
    #[cfg(feature = "anyproxy-ebpf")]
    let ebpf_tx = any_ebpf_core_conf.ebpf();
    use crate::config::socket_quic;
    let socket_quic_conf = socket_quic::main_conf(&ms).await;
    let domain_core_conf = domain_core::main_conf_mut(&ms).await;

    let value = value.get();

    let mut domain_config_context_map = HashMap::new();
    let mut index_map = HashMap::new();
    let mut index = 0;
    for domain_config_context in value.domain_config_contexts.iter() {
        let scc = domain_config_context.scc.get();
        let http_core_conf = http_core::currs_conf(scc.http_server_confs());

        index += 1;
        index_map.insert(index, (http_core_conf.domain.clone(), index));
        domain_config_context_map.insert(index, domain_config_context.clone());

        let mut index_map_test = HashMap::new();
        index_map_test.insert(index, (http_core_conf.domain.clone(), index));
        util::domain_index::DomainIndex::new(&index_map_test)
            .map_err(|e| anyhow!("err:domain => domain:{:?}, e:{}", http_core_conf.domain, e))?;
    }
    let domain_index = Arc::new(
        util::domain_index::DomainIndex::new(&index_map)
            .map_err(|e| anyhow!("err:domain => index_map:{:?}, e:{}", index_map, e))?,
    );
    let sni = quic::util::sni(&value.listens)?;

    let quic_config = {
        let scc = value.domain_config_contexts[0].scc.get();
        let http_core_conf0 = http_core::currs_conf(scc.http_server_confs());
        socket_quic_conf
            .config(&http_core_conf0.quic_config_name)
            .unwrap()
    };
    let listen_server: Arc<Box<dyn Server>> = Arc::new(Box::new(quic_server::Server::new(
        value.listen_addr.clone().unwrap(),
        common_core_conf.reuseport,
        quic_config,
        sni.clone(),
        #[cfg(feature = "anyproxy-ebpf")]
        ebpf_tx,
    )?));

    let plugin_handle_protocol = {
        use crate::config::http_server_core_plugin;
        let scc = value.domain_config_contexts[0].scc.get();
        let http_server_core_plugin_conf0 =
            http_server_core_plugin::currs_conf(scc.http_server_confs());
        http_server_core_plugin_conf0.plugin_handle_protocol.clone()
    };

    domain_core_conf.domain_config_listen_map.insert(
        key.clone(),
        DomainConfigListen {
            listen_server,
            domain_config_context_map,
            domain_index,
            sni: Some(sni),
            plugin_handle_protocol,
        },
    );
    Ok(())
}
