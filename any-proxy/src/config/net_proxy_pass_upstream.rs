use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::{anyhow, Context};
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::collections::HashMap;
use crate::config::config_toml::{ProxyPassTcp, default_tcp_config_name, ProxyPassSsl, ProxyPassQuic, default_quic_config_name, ProxyPassTcpTunnel2, ProxyPassSslTunnel2, ProxyPassQuicTunnel2, ProxyPassTcpTunnel, Tunnel, ProxyPassSslTunnel, ProxyPassQuicTunnel};
use crate::config::net_proxy_pass_tcp::do_proxy_pass_tcp;
use crate::util::util::host_and_port;
use crate::config::net_proxy_pass_ssl::do_proxy_pass_ssl;
use crate::config::net_proxy_pass_quic::do_proxy_pass_quic;
use crate::config::net_proxy_pass_tunnel2::{do_proxy_pass_tunnel2_tcp, do_proxy_pass_tunnel2_ssl, do_proxy_pass_tunnel2_quic};
use crate::config::net_proxy_pass_tunnel::{do_proxy_pass_tunnel_tcp, do_proxy_pass_tunnel_ssl, do_proxy_pass_tunnel_quic};

pub struct Conf {}

impl Conf {
    pub fn new() -> Self {
        Conf {}
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "proxy_pass_upstream".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_upstream(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_LOCAL,
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
        name: "net_proxy_pass_upstream".to_string(),
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

async fn proxy_pass_upstream(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();

    let main_protocol = HashMap::from([
        ("tcp", true),
        ("ssl", true),
        ("quic", true),
    ]);

    let main_protocol_tunnel = HashMap::from([
        ("tunnel", true),
    ]);

    let main_protocol_tunnel2 = HashMap::from([
        ("tunnel2", true),
    ]);

    let sub_protocol = HashMap::from([
        ("tcp", true),
        ("ssl", true),
        ("quic", true),
    ]);

    let ret:anyhow::Result<()> = async {
        let str = conf_arg.value.get::<String>();
        log::trace!(target: "main", "proxy_pass_upstream simple str:{:?}", str);
        let str = str.trim();
        if str.len() < 2 {
            return Ok(());
        }
        if &str[0..2] != "//" {
            return Ok(());
        }
        let str: &str = &str[2..];
        let strs = str.split("//").collect::<Vec<&str>>();
        if strs.len() < 2 {
            return Ok(());
        }
        let str0 = strs[0].trim();
        let str1 = strs[1].trim();
        if main_protocol.contains_key(str0) {
            if strs.len() != 3 {
                return Ok(());
            }
        } else if main_protocol_tunnel.contains_key(str0) {
            if strs.len() != 4 && strs.len() != 5 {
                return Ok(());
            }
            if !sub_protocol.contains_key(str1) {
                return Ok(());
            }
        } else if main_protocol_tunnel2.contains_key(str0) {
            if strs.len() != 4 {
                return Ok(());
            }
            if !sub_protocol.contains_key(str1) {
                return Ok(());
            }
        }

        if str0 == "tcp" {
            let address = strs[1].trim();
            let balancer = strs[2].trim();
            let proxy_pass_conf =  ProxyPassTcp {
                 balancer: balancer.to_string(),
                 address: address.to_string(), //ip:port, domain:port
                 tcp_config_name: default_tcp_config_name(),
                 heartbeat: None,
                 dynamic_domain: None,
                 is_proxy_protocol_hello: None,
                 weight: None,
            };
            do_proxy_pass_tcp(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
            return Err(anyhow::anyhow!("")).here_set_code(1);
        } else if str0 == "ssl" {
            let address = strs[1].trim();
            let address_split = address.split_once(",");
            let  (address, ssl_domain) = if address_split.is_none() {
                let (domain, _) = host_and_port(address);
                (address, domain)
            } else {
                let (address, ssl_domain) = address_split.unwrap();
                if ssl_domain.len() <= 0 {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    (address, ssl_domain)
                }
            };
            let balancer = strs[2].trim();
            let proxy_pass_conf =  ProxyPassSsl {
                balancer: balancer.to_string(),
                ssl_domain: ssl_domain.to_string(),
                address: address.to_string(), //ip:port, domain:port
                tcp_config_name: default_tcp_config_name(),
                heartbeat: None,
                dynamic_domain: None,
                is_proxy_protocol_hello: None,
                weight: None,
            };
            do_proxy_pass_ssl(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
            return Err(anyhow::anyhow!("")).here_set_code(1);
        } else if str0 == "quic" {
            let address = strs[1].trim();
            let address_split = address.split_once(",");
            let  (address, ssl_domain) = if address_split.is_none() {
                let (domain, _) = host_and_port(address);
                (address, domain)
            } else {
                let (address, ssl_domain) = address_split.unwrap();
                if ssl_domain.len() <= 0 {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    (address, ssl_domain)
                }
            };
            let balancer = strs[2].trim();
            let proxy_pass_conf =  ProxyPassQuic{
                balancer: balancer.to_string(),
                ssl_domain: ssl_domain.to_string(),
                address: address.to_string(), //ip:port, domain:port
                quic_config_name: default_quic_config_name(),
                heartbeat: None,
                dynamic_domain: None,
                is_proxy_protocol_hello: None,
                weight: None,
            };
            do_proxy_pass_quic(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
            return Err(anyhow::anyhow!("")).here_set_code(1);
        }else if str0 == "tunnel" {
            let tunnel = if strs.len() != 4 {
                Tunnel::new()
            } else {
                let tunnel = strs[4].trim();
                let tunnels = tunnel.split(",").collect::<Vec<&str>>();
                if tunnels.len() != 3 {
                    return Err(anyhow::anyhow!("tunnels.len() != 3"));
                }
                let max_stream_size: usize = tunnels[0].trim().parse().context("")?;
                let min_stream_cache_size: usize = tunnels[1].trim().parse().context("")?;
                let channel_size: usize = tunnels[2].trim().parse().context("")?;
                Tunnel {
                    max_stream_size,
                    min_stream_cache_size,
                    channel_size,
                }
            };
            if str1 == "tcp" {
                let address = strs[2].trim();
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassTcpTunnel {
                    balancer: balancer.to_string(),
                    tunnel,
                    address: address.to_string(), //ip:port, domain:port
                    tcp_config_name: default_tcp_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel_tcp(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else if str1 == "ssl" {
                let address = strs[2].trim();
                let address_split = address.split_once(",");
                let  (address, ssl_domain) = if address_split.is_none() {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    let (address, ssl_domain) = address_split.unwrap();
                    if ssl_domain.len() <= 0 {
                        let (domain, _) = host_and_port(address);
                        (address, domain)
                    } else {
                        (address, ssl_domain)
                    }
                };
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassSslTunnel {
                    balancer: balancer.to_string(),
                    tunnel,
                    ssl_domain: ssl_domain.to_string(),
                    address: address.to_string(), //ip:port, domain:port
                    tcp_config_name: default_tcp_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel_ssl(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else if str1 == "quic" {
                let address = strs[2].trim();
                let address_split = address.split_once(",");
                let  (address, ssl_domain) = if address_split.is_none() {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    let (address, ssl_domain) = address_split.unwrap();
                    if ssl_domain.len() <= 0 {
                        let (domain, _) = host_and_port(address);
                        (address, domain)
                    } else {
                        (address, ssl_domain)
                    }
                };
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassQuicTunnel{
                    balancer: balancer.to_string(),
                    tunnel,
                    ssl_domain: ssl_domain.to_string(),
                    address: address.to_string(), //ip:port, domain:port
                    quic_config_name: default_quic_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel_quic(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else {
                return Ok(());
            }
        }else if str0 == "tunnel2" {
            if str1 == "tcp" {
                let address = strs[2].trim();
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassTcpTunnel2 {
                    balancer: balancer.to_string(),
                    address: address.to_string(), //ip:port, domain:port
                    tcp_config_name: default_tcp_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel2_tcp(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else if str1 == "ssl" {
                let address = strs[2].trim();
                let address_split = address.split_once(",");
                let  (address, ssl_domain) = if address_split.is_none() {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    let (address, ssl_domain) = address_split.unwrap();
                    if ssl_domain.len() <= 0 {
                        let (domain, _) = host_and_port(address);
                        (address, domain)
                    } else {
                        (address, ssl_domain)
                    }
                };
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassSslTunnel2 {
                    balancer: balancer.to_string(),
                    ssl_domain: ssl_domain.to_string(),
                    address: address.to_string(), //ip:port, domain:port
                    tcp_config_name: default_tcp_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel2_ssl(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else if str1 == "quic" {
                let address = strs[2].trim();
                let address_split = address.split_once(",");
                let  (address, ssl_domain) = if address_split.is_none() {
                    let (domain, _) = host_and_port(address);
                    (address, domain)
                } else {
                    let (address, ssl_domain) = address_split.unwrap();
                    if ssl_domain.len() <= 0 {
                        let (domain, _) = host_and_port(address);
                        (address, domain)
                    } else {
                        (address, ssl_domain)
                    }
                };
                let balancer = strs[3].trim();
                let proxy_pass_conf =  ProxyPassQuicTunnel2{
                    balancer: balancer.to_string(),
                    ssl_domain: ssl_domain.to_string(),
                    address: address.to_string(), //ip:port, domain:port
                    quic_config_name: default_quic_config_name(),
                    heartbeat: None,
                    dynamic_domain: None,
                    is_proxy_protocol_hello: None,
                    weight: None,
                };
                do_proxy_pass_tunnel2_quic(&ms, &conf_arg, &cmd, proxy_pass_conf).await?;
                return Err(anyhow::anyhow!("")).here_set_code(1);
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        }

    }.await;
    if let Err(e) = ret {
        if e.code() > 0 {
            return Ok(());
        } else {
            return Err(e);
        }
    }

    let str = conf_arg.value.get::<String>();
    log::trace!(target: "main", "proxy_pass_upstream str:{:?}", str);
    use crate::config::upstream_core;
    let upstream_core_conf = upstream_core::main_conf_mut(&ms).await;
    let upstream_data = upstream_core_conf.upstream_data(str);
    if upstream_data.is_none() {
        return Err(anyhow!("err:{}", str));
    }

    use crate::config::net_server_core;
    let net_server_core_conf = net_server_core::curr_conf_mut(conf_arg.curr_conf());
    if net_server_core_conf.upstream_name.len() > 0 {
        return Err(anyhow!("err:str:{}", str));
    }
    net_server_core_conf.upstream_name = str.to_string();
    net_server_core_conf.upstream_data = upstream_data.unwrap();

    return Ok(());
}
