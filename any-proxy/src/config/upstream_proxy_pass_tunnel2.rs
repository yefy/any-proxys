use crate::config as conf;
use crate::config::config_toml::ProxyPassQuicTunnel2;
use crate::config::config_toml::ProxyPassSslTunnel2;
use crate::config::config_toml::ProxyPassTcpTunnel2;
use crate::config::config_toml::UpstreamHeartbeat;
use crate::config::upstream_block;
use crate::config::upstream_core;
use crate::stream::connect;
use crate::tunnel2::connect as tunnel2_connect;
use crate::upstream::UpstreamHeartbeatData;
use any_base::module::module;
use any_base::typ;
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
            name: "proxy_pass_tunnel2_tcp".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel2_tcp(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_pass_tunnel2_ssl".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel2_ssl(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_pass_tunnel2_quic".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_pass_tunnel2_quic(
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
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "upstream_proxy_pass_tunnel2".to_string(),
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

async fn proxy_pass_tunnel2_tcp(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassTcpTunnel2 =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassTcpTunnel2 proxy_pass_conf:{:?}", proxy_pass_conf);

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

async fn proxy_pass_tunnel2_ssl(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassSslTunnel2 =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassSslTunnel2 proxy_pass_conf:{:?}", proxy_pass_conf);

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

async fn proxy_pass_tunnel2_quic(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<upstream_block::Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_pass_conf: ProxyPassQuicTunnel2 =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    log::trace!(target: "main", "ProxyPassQuicTunnel2 proxy_pass_conf:{:?}", proxy_pass_conf);

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
    tcp: ProxyPassTcpTunnel2,
}

impl HeartbeatTcp {
    pub fn new(tcp: ProxyPassTcpTunnel2) -> Self {
        HeartbeatTcp { tcp }
    }
}

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
        use crate::config::tunnel2_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel2_core_conf = tunnel2_core::main_conf(&ms).await;

        let tcp_config_name = &self.tcp.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel2_core_conf.client();
        if client.is_none() {
            return Err(anyhow!("err:client is nil"));
        }
        let client = client.unwrap();
        let connect = Box::new(tunnel2_connect::Connect::new(
            client,
            Box::new(tunnel2_connect::PeerStreamConnectTcp::new(
                host,
                addr.clone(),
                tcp_config.unwrap(),
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
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}

pub struct HeartbeatSsl {
    ssl: ProxyPassSslTunnel2,
}

impl HeartbeatSsl {
    pub fn new(ssl: ProxyPassSslTunnel2) -> Self {
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
        use crate::config::tunnel2_core;
        let upstream_tcp_conf = socket_tcp::main_conf(&ms).await;
        let tunnel2_core_conf = tunnel2_core::main_conf(&ms).await;

        let tcp_config_name = &self.ssl.tcp_config_name;
        let tcp_config = upstream_tcp_conf.tcp_confs.get(tcp_config_name).cloned();
        if tcp_config.is_none() {
            return Err(anyhow!(
                "err:tcp_config_name => tcp_config_name:{}",
                tcp_config_name
            ));
        }
        let client = tunnel2_core_conf.client();
        if client.is_none() {
            return Err(anyhow!("err:client is nil"));
        }
        let client = client.unwrap();
        let connect = Box::new(tunnel2_connect::Connect::new(
            client,
            Box::new(tunnel2_connect::PeerStreamConnectSsl::new(
                host,
                addr.clone(),
                self.ssl.ssl_domain.clone(),
                tcp_config.unwrap(),
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
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}

pub struct HeartbeatQuic {
    quic: ProxyPassQuicTunnel2,
}

impl HeartbeatQuic {
    pub fn new(quic: ProxyPassQuicTunnel2) -> Self {
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
        use crate::config::tunnel2_core;
        let socket_quic_conf = socket_quic::main_conf(&ms).await;
        let tunnel2_core_conf = tunnel2_core::main_conf(&ms).await;

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
        let client = tunnel2_core_conf.client();
        if client.is_none() {
            return Err(anyhow!("err:client is nil"));
        }
        let client = client.unwrap();
        let connect = Box::new(tunnel2_connect::Connect::new(
            client,
            Box::new(tunnel2_connect::PeerStreamConnectQuic::new(
                host,
                addr.clone(),
                self.quic.ssl_domain.clone(),
                endpoints,
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
        };

        Ok(ShareRw::new(ups_heartbeat))
    }
}
