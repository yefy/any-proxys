use crate::config as conf;
use crate::proxy::stream_info::StreamInfo;
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcUnsafeAny, OptionExt, Share};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::net::IpAddr;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ConfIpv4 {
    pub data: String,
    pub ip: String,
    pub ip_bin: u32,
    pub offset: u32,
    pub is_deny: bool,
}

#[derive(Debug, Clone)]
pub struct ConfIpv6 {
    pub data: String,
    pub ip: String,
    pub ip_bin: u128,
    pub offset: u32,
    pub is_deny: bool,
}

#[derive(Debug, Clone)]
pub struct ConfIp {
    pub ipv4: Vec<ConfIpv4>,
    pub ipv6: Vec<ConfIpv6>,
    pub is_deny_all: bool,
}

impl ConfIp {
    pub fn new() -> Self {
        ConfIp {
            ipv4: Vec::with_capacity(10),
            ipv6: Vec::with_capacity(10),
            is_deny_all: false,
        }
    }
}

pub struct Conf {
    pub ip: OptionExt<ConfIp>,
}

impl Conf {
    pub async fn new() -> Result<Self> {
        Ok(Conf { ip: None.into() })
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "ip_deny".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(ip_deny(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "ip_allow".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(ip_allow(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        }
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
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_access_ip".to_string(),
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
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new().await?)));
}

async fn merge_conf(
    _ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let child_conf = child_conf.get_mut::<Conf>();
    if parent_conf.is_some() {
        let parent_conf = parent_conf.unwrap();
        let parent_conf = parent_conf.get_mut::<Conf>();

        if child_conf.ip.is_none() {
            child_conf.ip = parent_conf.ip.clone();
        }
    }

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
    use crate::config::net_core_plugin;
    let net_core_plugin_conf = net_core_plugin::main_conf_mut(&ms).await;
    net_core_plugin_conf
        .plugin_handle_access
        .get_mut()
        .await
        .push(|stream_info| Box::pin(access(stream_info)));
    net_core_plugin_conf
        .plugin_handle_http_access
        .get_mut()
        .await
        .push(|stream_info| Box::pin(access(stream_info)));
    return Ok(());
}

async fn ip_allow(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    ip_add(conf_arg, conf, false).await
}

async fn ip_deny(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    ip_add(conf_arg, conf, true).await
}

async fn ip_add(conf_arg: module::ConfArg, conf: typ::ArcUnsafeAny, is_deny: bool) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>().as_str();
    let strs = str.rsplit_once("/");
    let (ip_str, offset) = if strs.is_none() {
        (str, None)
    } else {
        let (ip, offset) = strs.unwrap();
        (ip, Some(offset))
    };

    if c.ip.is_none() {
        c.ip = Some(ConfIp::new()).into();
    }
    if ip_str == "all" {
        c.ip.is_deny_all = true;
    } else {
        use std::str::FromStr;
        let ip = IpAddr::from_str(ip_str)
            .map_err(|e| anyhow!("err:ip_allow => addr:{}, e:{}", str, e))?;
        match ip {
            IpAddr::V4(ipv4) => {
                let offset = if offset.is_none() {
                    0
                } else {
                    let offset = offset
                        .unwrap()
                        .parse::<u32>()
                        .map_err(|e| anyhow!("err:ip_allow => addr:{}, e:{}", str, e))?;
                    if offset <= 0 || offset > 32 {
                        return Err(anyhow!("err:ip_allow => addr:{}", str));
                    }
                    32 - offset
                };
                c.ip.ipv4.push(ConfIpv4 {
                    data: str.to_string(),
                    ip: ip_str.to_string(),
                    ip_bin: u32::from_be_bytes(ipv4.octets()),
                    offset,
                    is_deny,
                });
            }
            IpAddr::V6(ipv6) => {
                let offset = if offset.is_none() {
                    0
                } else {
                    let offset = offset
                        .unwrap()
                        .parse::<u32>()
                        .map_err(|e| anyhow!("err:ip_allow => addr:{}, e:{}", str, e))?;
                    if offset <= 0 || offset > 128 {
                        return Err(anyhow!("err:ip_allow => addr:{}", str));
                    }
                    128 - offset
                };
                c.ip.ipv6.push(ConfIpv6 {
                    data: str.to_string(),
                    ip: ip_str.to_string(),
                    ip_bin: u128::from_be_bytes(ipv6.octets()),
                    offset,
                    is_deny,
                });
            }
        }
    }
    if c.ip.is_some() {
        log::trace!(target: "main", "c.ip:{:?}", c.ip.as_ref());
    }
    return Ok(());
}

pub async fn access(stream_info: Share<StreamInfo>) -> Result<crate::Error> {
    let scc = stream_info.get().scc.clone();
    if scc.is_none() {
        return Ok(crate::Error::Ok);
    }

    use crate::config::net_access_ip;
    let net_access_ip_conf = net_access_ip::curr_conf(scc.net_curr_conf());
    if net_access_ip_conf.ip.is_none() {
        return Ok(crate::Error::Ok);
    }
    let remote_addr = stream_info.get().server_stream_info.remote_addr.clone();
    let ip = remote_addr.ip();
    match ip {
        IpAddr::V4(ip) => {
            let ip_bin = u32::from_be_bytes(ip.octets());
            for ipv4 in net_access_ip_conf.ip.ipv4.iter() {
                if ip_bin >> ipv4.offset == ipv4.ip_bin >> ipv4.offset {
                    if ipv4.is_deny {
                        return Ok(crate::Error::Return);
                    } else {
                        return Ok(crate::Error::Ok);
                    }
                }
            }
        }
        IpAddr::V6(ip) => {
            let ip_bin = u128::from_be_bytes(ip.octets());
            for ipv6 in net_access_ip_conf.ip.ipv6.iter() {
                if ip_bin >> ipv6.offset == ipv6.ip_bin >> ipv6.offset {
                    if ipv6.is_deny {
                        return Ok(crate::Error::Return);
                    } else {
                        return Ok(crate::Error::Ok);
                    }
                }
            }
        }
    }
    if net_access_ip_conf.ip.is_deny_all {
        return Ok(crate::Error::Return);
    }
    return Ok(crate::Error::Ok);
}
