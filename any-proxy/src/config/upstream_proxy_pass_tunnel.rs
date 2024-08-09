use crate::config as conf;
use crate::config::config_toml::ProxyPassQuicTunnel;
use crate::config::config_toml::ProxyPassSslTunnel;
use crate::config::config_toml::ProxyPassTcpTunnel;
use crate::config::config_toml::UpstreamHeartbeat;
use crate::config::upstream_block;
use crate::config::upstream_core;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::connect;
use crate::tunnel::connect as tunnel_connect;
use crate::upstream::UpstreamHeartbeatData;
use crate::util::var::Var;
use any_base::module::module;
use any_base::typ;
use any_base::typ::Share;
use any_base::typ::{ArcUnsafeAny, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use module::Modules;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

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
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_pass_tunnel_ssl".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel_ssl(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_pass_tunnel_quic".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel_quic(
                ms, conf_arg, cmd, conf
            )),
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
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "upstream_proxy_pass_tunnel".to_string(),
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

async fn proxy_pass_tunnel_tcp(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassTcpTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassTcpTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> =
        Arc::new(Box::new(HeartbeatTcp::new(proxy_pass_conf.clone())));

    conf.add_proxy_pass(
        ms.clone(),
        proxy_pass_conf.address.clone(),
        proxy_pass_conf.dynamic_domain.clone(),
        heartbeat,
    )
    .await?;

    return Ok(());
}

async fn proxy_pass_tunnel_ssl(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassSslTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassSslTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> =
        Arc::new(Box::new(HeartbeatSsl::new(proxy_pass_conf.clone())));

    conf.add_proxy_pass(
        ms.clone(),
        proxy_pass_conf.address.clone(),
        proxy_pass_conf.dynamic_domain.clone(),
        heartbeat,
    )
    .await?;

    return Ok(());
}

async fn proxy_pass_tunnel_quic(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassQuicTunnel =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassQuicTunnel proxy_pass_conf:{:?}", proxy_pass_conf);

    let heartbeat: Arc<Box<dyn upstream_core::HeartbeatI>> =
        Arc::new(Box::new(HeartbeatQuic::new(proxy_pass_conf.clone())));

    conf.add_proxy_pass(
        ms.clone(),
        proxy_pass_conf.address.clone(),
        proxy_pass_conf.dynamic_domain.clone(),
        heartbeat,
    )
    .await?;

    return Ok(());
}

pub struct HeartbeatTcp {
    tcp: ProxyPassTcpTunnel,
}

impl HeartbeatTcp {
    pub fn new(tcp: ProxyPassTcpTunnel) -> Self {
        HeartbeatTcp { tcp }
    }
}

use crate::config::upstream_proxy_pass_tcp::http_heartbeat;
use async_trait::async_trait;

#[async_trait]
impl upstream_core::HeartbeatI for HeartbeatTcp {
    async fn heartbeat(
        &self,
        ms: Modules,
        domain_index: usize,
        ups_heartbeats_index: usize,
        addr: &SocketAddr,
        host: String,
        ups_heartbeat: Option<UpstreamHeartbeat>,
        is_weight: bool,
    ) -> Result<ShareRw<UpstreamHeartbeatData>> {
        use crate::config::socket_tcp;
        use crate::config::tunnel_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let tcp_config_name = &self.tcp.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }
        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectTcp::new(
                host.clone(),
                addr.clone(),
                tcp_config.unwrap(),
                self.tcp.tunnel.max_stream_size,
                self.tcp.tunnel.min_stream_cache_size,
                self.tcp.tunnel.channel_size,
            )),
        ));
        let heartbeat = self.tcp.heartbeat.clone();
        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.tcp.is_proxy_protocol_hello;
        let weight = self.tcp.weight;

        let heartbeat = if heartbeat.is_some() {
            heartbeat
        } else {
            ups_heartbeat
        };

        if is_weight && weight.is_none() {
            return Err(anyhow!("err:weight nil, proxy_pass={:?}", self.tcp));
        }

        let weight = if weight.is_some() { weight.unwrap() } else { 1 };

        let http_heartbeat = if heartbeat.is_some() {
            let heartbeat = heartbeat.clone().unwrap();
            if heartbeat.http.is_some() {
                let http = heartbeat.http.as_ref().unwrap();
                let http_heartbeat = http_heartbeat(host.into(), addr.clone(), &ms, http).await?;
                Some(http_heartbeat)
            } else {
                None
            }
        } else {
            None
        };

        let (shutdown_heartbeat_tx, _) = broadcast::channel(100);
        let ups_heartbeat = UpstreamHeartbeatData {
            domain_index,
            index: ups_heartbeats_index,
            heartbeat,
            addr: addr.clone(),
            connect,
            curr_fail: 0,
            disable: false,
            shutdown_heartbeat_tx,
            is_proxy_protocol_hello,
            total_elapsed: 0,
            count_elapsed: 0,
            avg_elapsed: 0,
            weight,
            effective_weight: weight,
            current_weight: 0,
            http_heartbeat,
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}

pub struct HeartbeatSsl {
    ssl: ProxyPassSslTunnel,
}

impl HeartbeatSsl {
    pub fn new(ssl: ProxyPassSslTunnel) -> Self {
        HeartbeatSsl { ssl }
    }
}

#[async_trait]
impl upstream_core::HeartbeatI for HeartbeatSsl {
    async fn heartbeat(
        &self,
        ms: Modules,
        domain_index: usize,
        ups_heartbeats_index: usize,
        addr: &SocketAddr,
        host: String,
        ups_heartbeat: Option<UpstreamHeartbeat>,
        is_weight: bool,
    ) -> Result<ShareRw<UpstreamHeartbeatData>> {
        use crate::config::socket_tcp;
        use crate::config::tunnel_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let tcp_config_name = &self.ssl.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }
        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectSsl::new(
                host.clone(),
                addr.clone(),
                self.ssl.ssl_domain.clone(),
                tcp_config.unwrap(),
                self.ssl.tunnel.max_stream_size,
                self.ssl.tunnel.min_stream_cache_size,
                self.ssl.tunnel.channel_size,
            )),
        ));

        let heartbeat = self.ssl.heartbeat.clone();
        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.ssl.is_proxy_protocol_hello;
        let weight = self.ssl.weight;

        let heartbeat = if heartbeat.is_some() {
            heartbeat
        } else {
            ups_heartbeat
        };

        if is_weight && weight.is_none() {
            return Err(anyhow!("err:weight nil, proxy_pass={:?}", self.ssl));
        }

        let weight = if weight.is_some() { weight.unwrap() } else { 1 };

        let http_heartbeat = if heartbeat.is_some() {
            let heartbeat = heartbeat.clone().unwrap();
            if heartbeat.http.is_some() {
                let http = heartbeat.http.as_ref().unwrap();
                let http_heartbeat = http_heartbeat(host.into(), addr.clone(), &ms, http).await?;
                Some(http_heartbeat)
            } else {
                None
            }
        } else {
            None
        };

        let (shutdown_heartbeat_tx, _) = broadcast::channel(100);
        let ups_heartbeat = UpstreamHeartbeatData {
            domain_index,
            index: ups_heartbeats_index,
            heartbeat,
            addr: addr.clone(),
            connect,
            curr_fail: 0,
            disable: false,
            shutdown_heartbeat_tx,
            is_proxy_protocol_hello,
            total_elapsed: 0,
            count_elapsed: 0,
            avg_elapsed: 0,
            weight,
            effective_weight: weight,
            current_weight: 0,
            http_heartbeat,
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}

pub struct HeartbeatQuic {
    quic: ProxyPassQuicTunnel,
}

impl HeartbeatQuic {
    pub fn new(quic: ProxyPassQuicTunnel) -> Self {
        HeartbeatQuic { quic }
    }
}

#[async_trait]
impl upstream_core::HeartbeatI for HeartbeatQuic {
    async fn heartbeat(
        &self,
        ms: Modules,
        domain_index: usize,
        ups_heartbeats_index: usize,
        addr: &SocketAddr,
        host: String,
        ups_heartbeat: Option<UpstreamHeartbeat>,
        is_weight: bool,
    ) -> Result<ShareRw<UpstreamHeartbeatData>> {
        use crate::config::socket_quic;
        use crate::config::tunnel_core;
        let socket_quic_conf = socket_quic::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let quic_config_name = &self.quic.quic_config_name;
        let quic_config = socket_quic_conf.quic_confs.get(quic_config_name).cloned();
        if quic_config.is_none() {
            return Err(anyhow!(
                "err:quic_config_name => quic_config_name:{}",
                quic_config_name
            ));
        }
        let endpoints = socket_quic_conf
            .endpoints_map
            .get(quic_config_name)
            .cloned()
            .unwrap();
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }
        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectQuic::new(
                host.clone(),
                addr.clone(),
                self.quic.ssl_domain.clone(),
                endpoints,
                self.quic.tunnel.max_stream_size,
                self.quic.tunnel.min_stream_cache_size,
                self.quic.tunnel.channel_size,
                quic_config.unwrap(),
            )),
        ));

        let heartbeat = self.quic.heartbeat.clone();
        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.quic.is_proxy_protocol_hello;
        let weight = self.quic.weight;

        let heartbeat = if heartbeat.is_some() {
            heartbeat
        } else {
            ups_heartbeat
        };

        if is_weight && weight.is_none() {
            return Err(anyhow!("err:weight nil, proxy_pass={:?}", self.quic));
        }

        let weight = if weight.is_some() { weight.unwrap() } else { 1 };

        let http_heartbeat = if heartbeat.is_some() {
            let heartbeat = heartbeat.clone().unwrap();
            if heartbeat.http.is_some() {
                let http = heartbeat.http.as_ref().unwrap();
                let http_heartbeat = http_heartbeat(host.into(), addr.clone(), &ms, http).await?;
                Some(http_heartbeat)
            } else {
                None
            }
        } else {
            None
        };
        let (shutdown_heartbeat_tx, _) = broadcast::channel(100);
        let ups_heartbeat = UpstreamHeartbeatData {
            domain_index,
            index: ups_heartbeats_index,
            heartbeat,
            addr: addr.clone(),
            connect,
            curr_fail: 0,
            disable: false,
            shutdown_heartbeat_tx,
            is_proxy_protocol_hello,
            total_elapsed: 0,
            count_elapsed: 0,
            avg_elapsed: 0,
            weight,
            effective_weight: weight,
            current_weight: 0,
            http_heartbeat,
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}

pub struct UpstreamTcp {
    address_vars: Option<Var>,
    tcp: ProxyPassTcpTunnel,
}

impl UpstreamTcp {
    pub fn new(address_vars: Option<Var>, tcp: ProxyPassTcpTunnel) -> Self {
        UpstreamTcp { address_vars, tcp }
    }
}

#[async_trait]
impl upstream_core::GetConnectI for UpstreamTcp {
    async fn get_connect(
        &self,
        ms: &Modules,
        stream_info: &Share<StreamInfo>,
    ) -> Result<(Option<bool>, Arc<Box<dyn connect::Connect>>)> {
        use crate::proxy::stream_var;
        let address = if self.address_vars.is_some() {
            let stream_info = stream_info.get();
            let mut address_vars = Var::copy(self.address_vars.as_ref().unwrap())
                .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            address_vars.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &stream_info)
                    .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                Ok(value)
            })?;
            let address = address_vars
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            address
        } else {
            self.tcp.address.clone()
        };

        use crate::util;
        let timeout = if self.tcp.dynamic_domain.is_some() {
            self.tcp.dynamic_domain.as_ref().unwrap().timeout
        } else {
            5
        };
        let addr =
            util::util::lookup_host(tokio::time::Duration::from_secs(timeout as u64), &address)
                .await?;

        use crate::config::socket_tcp;
        use crate::config::tunnel_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let tcp_config_name = &self.tcp.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }

        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectTcp::new(
                address.clone(),
                addr.clone(),
                tcp_config.unwrap(),
                self.tcp.tunnel.max_stream_size,
                self.tcp.tunnel.min_stream_cache_size,
                self.tcp.tunnel.channel_size,
            )),
        ));

        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.tcp.is_proxy_protocol_hello;

        Ok((is_proxy_protocol_hello, connect))
    }
}

pub struct UpstreamSsl {
    address_vars: Option<Var>,
    ssl: ProxyPassSslTunnel,
}

impl UpstreamSsl {
    pub fn new(address_vars: Option<Var>, ssl: ProxyPassSslTunnel) -> Self {
        UpstreamSsl { address_vars, ssl }
    }
}

#[async_trait]
impl upstream_core::GetConnectI for UpstreamSsl {
    async fn get_connect(
        &self,
        ms: &Modules,
        stream_info: &Share<StreamInfo>,
    ) -> Result<(Option<bool>, Arc<Box<dyn connect::Connect>>)> {
        use crate::proxy::stream_var;
        let address = if self.address_vars.is_some() {
            let stream_info = stream_info.get();
            let mut address_vars = Var::copy(self.address_vars.as_ref().unwrap())
                .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            address_vars.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &stream_info)
                    .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                Ok(value)
            })?;
            let address = address_vars
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            address
        } else {
            self.ssl.address.clone()
        };

        use crate::util;
        let timeout = if self.ssl.dynamic_domain.is_some() {
            self.ssl.dynamic_domain.as_ref().unwrap().timeout
        } else {
            5
        };
        let addr =
            util::util::lookup_host(tokio::time::Duration::from_secs(timeout as u64), &address)
                .await?;

        use crate::config::socket_tcp;
        use crate::config::tunnel_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let tcp_config_name = &self.ssl.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }
        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectSsl::new(
                address.clone(),
                addr.clone(),
                self.ssl.ssl_domain.clone(),
                tcp_config.unwrap(),
                self.ssl.tunnel.max_stream_size,
                self.ssl.tunnel.min_stream_cache_size,
                self.ssl.tunnel.channel_size,
            )),
        ));
        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.ssl.is_proxy_protocol_hello;

        Ok((is_proxy_protocol_hello, connect))
    }
}

pub struct UpstreamQuic {
    address_vars: Option<Var>,
    quic: ProxyPassQuicTunnel,
}

impl UpstreamQuic {
    pub fn new(address_vars: Option<Var>, quic: ProxyPassQuicTunnel) -> Self {
        UpstreamQuic { address_vars, quic }
    }
}

#[async_trait]
impl upstream_core::GetConnectI for UpstreamQuic {
    async fn get_connect(
        &self,
        ms: &Modules,
        stream_info: &Share<StreamInfo>,
    ) -> Result<(Option<bool>, Arc<Box<dyn connect::Connect>>)> {
        use crate::proxy::stream_var;
        let address = if self.address_vars.is_some() {
            let stream_info = stream_info.get();
            let mut address_vars = Var::copy(self.address_vars.as_ref().unwrap())
                .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            address_vars.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &stream_info)
                    .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                Ok(value)
            })?;
            let address = address_vars
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            address
        } else {
            self.quic.address.clone()
        };

        use crate::util;
        let timeout = if self.quic.dynamic_domain.is_some() {
            self.quic.dynamic_domain.as_ref().unwrap().timeout
        } else {
            5
        };
        let addr =
            util::util::lookup_host(tokio::time::Duration::from_secs(timeout as u64), &address)
                .await?;

        use crate::config::socket_quic;
        use crate::config::tunnel_core;
        let socket_quic_conf = socket_quic::main_conf(&ms).await;
        let tunnel_conf = tunnel_core::main_conf(&ms).await;

        let quic_config_name = &self.quic.quic_config_name;
        let quic_config = socket_quic_conf.quic_confs.get(quic_config_name).cloned();
        if quic_config.is_none() {
            return Err(anyhow!(
                "err:quic_config_name => quic_config_name:{}",
                quic_config_name
            ));
        }
        let endpoints = socket_quic_conf
            .endpoints_map
            .get(quic_config_name)
            .cloned()
            .unwrap();
        let client = tunnel_conf.client();
        if client.is_none() {
            return Err(anyhow::anyhow!("err:tunnel_conf client nil"));
        }
        let connect = Box::new(tunnel_connect::Connect::new(
            client.unwrap(),
            Box::new(tunnel_connect::PeerStreamConnectQuic::new(
                address.clone(),
                addr.clone(),
                self.quic.ssl_domain.clone(),
                endpoints,
                self.quic.tunnel.max_stream_size,
                self.quic.tunnel.min_stream_cache_size,
                self.quic.tunnel.channel_size,
                quic_config.unwrap(),
            )),
        ));

        let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
        let is_proxy_protocol_hello = self.quic.is_proxy_protocol_hello;

        Ok((is_proxy_protocol_hello, connect))
    }
}
