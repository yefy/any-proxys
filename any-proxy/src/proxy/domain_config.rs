use super::access_log::AccessLog;
use super::proxy;
use super::stream_var;
use super::StreamConfigContext;
use crate::config::config_toml;
use crate::config::config_toml::Listen;
use crate::config::config_toml::SSL;
use crate::config::config_toml::{DomainListen, DomainListenType, ServerConfigType};
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::quic;
use crate::quic::server as quic_server;
use crate::ssl::server as ssl_server;
use crate::stream::server::Server;
use crate::tcp::server as tcp_server;
use crate::upstream::upstream;
use crate::util;
use crate::TunnelClients;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
#[cfg(feature = "anyproxy-ebpf")]
use std::sync::Arc;

pub struct DomainConfigContext {
    pub stream_config_context: Rc<StreamConfigContext>,
}

#[derive(Clone)]
pub struct DomainConfigListen {
    pub common: config_toml::CommonConfig,
    pub stream: config_toml::StreamConfig,
    pub listen_server: Rc<Box<dyn Server>>,
    pub domain_config_context_map: HashMap<i32, Rc<DomainConfigContext>>,
    pub domain_index: Rc<util::domain_index::DomainIndex>,
    pub sni: Option<util::Sni>,
    pub server_type: ServerConfigType,
}

#[derive(Clone)]
pub struct DomainConfigListenMerge {
    pub listen_type: DomainListenType,
    pub common: Option<config_toml::CommonConfig>,
    pub stream: Option<config_toml::StreamConfig>,
    pub tcp_config: Option<config_toml::TcpConfig>,
    pub quic_config: Option<config_toml::QuicConfig>,
    pub listen_addr: Option<SocketAddr>,
    pub listens: Vec<Listen>,
    pub domain_config_contexts: Vec<Rc<DomainConfigContext>>,
    pub server_type: ServerConfigType,
}

pub struct DomainConfig {}

impl DomainConfig {
    pub fn new() -> Result<DomainConfig> {
        Ok(DomainConfig {})
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        _tunnel_clients: TunnelClients,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Option<Arc<any_ebpf::AddSockHash>>,
    ) -> Result<HashMap<String, DomainConfigListen>> {
        let mut key_map = HashMap::new();
        let mut domain_config_listen_map = HashMap::new();
        let mut domain_config_listen_merge_map = HashMap::new();

        for domain_server_config in config._domain.as_ref().unwrap()._server.iter() {
            let access_context =
                AccessLog::parse_config_access_log(&domain_server_config.access).await?;

            let upstream_data = ups.upstream_data(&domain_server_config.proxy_pass_upstream);
            if upstream_data.is_none() {
                return Err(anyhow!(
                    "err:ups.upstream_data => proxy_pass_upstream:{}",
                    domain_server_config.proxy_pass_upstream
                ));
            }

            let tcp_str = domain_server_config.tcp.clone().take().unwrap();

            let tcp_config = ups.tcp_config(&tcp_str);
            if tcp_config.is_none() {
                return Err(anyhow!("err:ups.tcp_str => tcp_str:{}", tcp_str));
            }
            let tcp_config = tcp_config.unwrap();

            let quic_str = domain_server_config.quic.clone().take().unwrap();
            let quic_config = ups.quic_config(&quic_str);
            if quic_config.is_none() {
                return Err(anyhow!("err:ups.quic => quic_str:{}", quic_str));
            }
            let quic_config = quic_config.unwrap();

            let stream_config_context = Rc::new(StreamConfigContext {
                common: domain_server_config.common.clone().take().unwrap(),
                tcp: tcp_config.clone(),
                quic: quic_config.clone(),
                stream: domain_server_config.stream.clone().take().unwrap(),
                rate: domain_server_config.rate.clone().take().unwrap(),
                tmp_file: domain_server_config.tmp_file.clone().take().unwrap(),
                fast_conf: domain_server_config.fast_conf.clone().take().unwrap(),
                access: domain_server_config.access.clone().take().unwrap(),
                access_context: access_context.unwrap(),
                domain: Some(domain_server_config.domain.clone()),
                is_proxy_protocol_hello: domain_server_config.is_proxy_protocol_hello,
                ups_data: upstream_data.unwrap(),
                stream_var: Rc::new(stream_var::StreamVar::new()),
                server: domain_server_config.server.clone(),
            });
            let domain_config_context = Rc::new(DomainConfigContext {
                stream_config_context: stream_config_context.clone(),
            });

            for listen in domain_server_config.listen.as_ref().unwrap().iter() {
                let listen_type = listen.listen_type();
                match listen {
                    DomainListen::Tcp(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let key = util::util::tcp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:DomainConfig::tcp_key_from_addr => e:{}", e)
                                })?;
                                if domain_config_listen_merge_map.get(&key).is_none() {
                                    if key_map.get(&key).is_some() {
                                        return Err(anyhow!("err:key is exist => key:{}", key));
                                    }
                                    key_map.insert(key.clone(), true);
                                    domain_config_listen_merge_map.insert(
                                        key.clone(),
                                        DomainConfigListenMerge {
                                            common: None,
                                            stream: None,
                                            tcp_config: None,
                                            quic_config: None,
                                            listen_type,
                                            listen_addr: None,
                                            listens: Vec::new(),
                                            domain_config_contexts: Vec::new(),
                                            server_type: domain_config_context
                                                .stream_config_context
                                                .server_type(),
                                        },
                                    );
                                }

                                let mut values =
                                    domain_config_listen_merge_map.get_mut(&key).unwrap();
                                let listen = config_toml::Listen {
                                    address: addr.to_string(),
                                    ssl: None,
                                };

                                values.common = Some(stream_config_context.common.clone());
                                values.stream = Some(stream_config_context.stream.clone());
                                if values.tcp_config.is_none() {
                                    values.tcp_config = Some(stream_config_context.tcp.clone());
                                    values.quic_config = Some(stream_config_context.quic.clone());
                                }
                                values.listens.push(listen);
                                values.listen_addr = Some(addr.clone());
                                values
                                    .domain_config_contexts
                                    .push(domain_config_context.clone());

                                if values.server_type
                                    != domain_config_context.stream_config_context.server_type()
                                {
                                    return Err(anyhow!("err:listen => addr:{}", addr));
                                }
                            }
                            Ok(())
                        }
                        .await;
                        ret.map_err(|e| {
                            anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                    DomainListen::Ssl(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let key = util::util::tcp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:DomainConfig::tcp_key_from_addr => e:{}", e)
                                })?;
                                if domain_config_listen_merge_map.get(&key).is_none() {
                                    if key_map.get(&key).is_some() {
                                        return Err(anyhow!("err:key is exist => key:{}", key));
                                    }
                                    key_map.insert(key.clone(), true);
                                    domain_config_listen_merge_map.insert(
                                        key.clone(),
                                        DomainConfigListenMerge {
                                            common: None,
                                            stream: None,
                                            tcp_config: None,
                                            quic_config: None,
                                            listen_type,
                                            listen_addr: None,
                                            listens: Vec::new(),
                                            domain_config_contexts: Vec::new(),
                                            server_type: domain_config_context
                                                .stream_config_context
                                                .server_type(),
                                        },
                                    );
                                }

                                let mut values =
                                    domain_config_listen_merge_map.get_mut(&key).unwrap();

                                let ssl = SSL {
                                    ssl_domain: domain_server_config.domain.clone(),
                                    cert: listen.ssl.cert.clone(),
                                    key: listen.ssl.key.clone(),
                                    tls: listen.ssl.tls.clone(),
                                };
                                let listen = config_toml::Listen {
                                    address: addr.to_string(),
                                    ssl: Some(ssl),
                                };

                                values.common = Some(stream_config_context.common.clone());
                                values.stream = Some(stream_config_context.stream.clone());
                                if values.tcp_config.is_none() {
                                    values.tcp_config = Some(stream_config_context.tcp.clone());
                                    values.quic_config = Some(stream_config_context.quic.clone());
                                }
                                values.listens.push(listen);
                                values.listen_addr = Some(addr.clone());
                                values
                                    .domain_config_contexts
                                    .push(domain_config_context.clone());

                                if values.server_type
                                    != domain_config_context.stream_config_context.server_type()
                                {
                                    return Err(anyhow!("err:listen => addr:{}", addr));
                                }
                            }
                            Ok(())
                        }
                        .await;
                        ret.map_err(|e| {
                            anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                    DomainListen::Quic(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let udp_key = util::util::udp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:DomainConfig::udp_key_from_addr => e:{}", e)
                                })?;
                                let quic_key =
                                    util::util::quic_key_from_addr(addr).map_err(|e| {
                                        anyhow!("err:DomainConfig::quic_key_from_addr => e:{}", e)
                                    })?;
                                if domain_config_listen_merge_map.get(&quic_key).is_none() {
                                    if key_map.get(&udp_key).is_some() {
                                        return Err(anyhow!(
                                            "err:udp_key is exist => key:{}",
                                            udp_key
                                        ));
                                    }
                                    key_map.insert(udp_key.clone(), true);
                                    domain_config_listen_merge_map.insert(
                                        quic_key.clone(),
                                        DomainConfigListenMerge {
                                            common: None,
                                            stream: None,
                                            tcp_config: None,
                                            quic_config: None,
                                            listen_type,
                                            listen_addr: None,
                                            listens: Vec::new(),
                                            domain_config_contexts: Vec::new(),
                                            server_type: domain_config_context
                                                .stream_config_context
                                                .server_type(),
                                        },
                                    );
                                }

                                let mut values =
                                    domain_config_listen_merge_map.get_mut(&quic_key).unwrap();

                                let ssl = SSL {
                                    ssl_domain: domain_server_config.domain.clone(),
                                    cert: listen.ssl.cert.clone(),
                                    key: listen.ssl.key.clone(),
                                    tls: listen.ssl.tls.clone(),
                                };
                                let listen = config_toml::Listen {
                                    address: addr.to_string(),
                                    ssl: Some(ssl),
                                };

                                values.common = Some(stream_config_context.common.clone());
                                values.stream = Some(stream_config_context.stream.clone());
                                if values.tcp_config.is_none() {
                                    values.tcp_config = Some(stream_config_context.tcp.clone());
                                    values.quic_config = Some(stream_config_context.quic.clone());
                                }
                                values.listen_addr = Some(addr.clone());
                                values.listens.push(listen);
                                values
                                    .domain_config_contexts
                                    .push(domain_config_context.clone());

                                if values.server_type
                                    != domain_config_context.stream_config_context.server_type()
                                {
                                    return Err(anyhow!("err:listen => addr:{}", addr));
                                }
                            }
                            Ok(())
                        }
                        .await;
                        ret.map_err(|e| {
                            anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                }
            }
        }

        for (key, value) in domain_config_listen_merge_map.iter_mut() {
            match &value.listen_type {
                &DomainListenType::Tcp => {
                    let mut domain_config_context_map = HashMap::new();
                    let mut index_map = HashMap::new();
                    let mut index = 0;
                    for domain_config_context in value.domain_config_contexts.iter() {
                        let stream_config_context =
                            domain_config_context.stream_config_context.clone();
                        index += 1;
                        index_map.insert(
                            index,
                            (stream_config_context.domain.clone().unwrap(), index),
                        );
                        domain_config_context_map.insert(index, domain_config_context.clone());

                        let mut index_map_test = HashMap::new();
                        index_map_test.insert(
                            index,
                            (stream_config_context.domain.clone().unwrap(), index),
                        );
                        util::domain_index::DomainIndex::new(&index_map_test).map_err(|e| {
                            anyhow!(
                                "err:domain => domain:{:?}, e:{}",
                                stream_config_context.domain,
                                e
                            )
                        })?;
                    }
                    let domain_index = Rc::new(
                        util::domain_index::DomainIndex::new(&index_map).map_err(|e| {
                            anyhow!("err:domain => index_map:{:?}, e:{}", index_map, e)
                        })?,
                    );

                    let listen_server: Rc<Box<dyn Server>> =
                        Rc::new(Box::new(tcp_server::Server::new(
                            value.listen_addr.clone().unwrap(),
                            value.common.as_ref().unwrap().reuseport,
                            value.tcp_config.clone().unwrap(),
                        )?));

                    domain_config_listen_map.insert(
                        key.clone(),
                        DomainConfigListen {
                            common: value.common.clone().unwrap(),
                            stream: value.stream.clone().unwrap(),
                            listen_server,
                            domain_config_context_map,
                            domain_index,
                            sni: None,
                            server_type: value.server_type,
                        },
                    );
                }
                &DomainListenType::Ssl => {
                    let mut domain_config_context_map = HashMap::new();
                    let mut index_map = HashMap::new();
                    let mut index = 0;
                    for domain_config_context in value.domain_config_contexts.iter() {
                        index += 1;
                        index_map.insert(
                            index,
                            (
                                domain_config_context
                                    .stream_config_context
                                    .domain
                                    .clone()
                                    .unwrap(),
                                index,
                            ),
                        );
                        domain_config_context_map.insert(index, domain_config_context.clone());

                        let mut index_map_test = HashMap::new();
                        index_map_test.insert(
                            index,
                            (
                                domain_config_context
                                    .stream_config_context
                                    .domain
                                    .clone()
                                    .unwrap(),
                                index,
                            ),
                        );
                        util::domain_index::DomainIndex::new(&index_map_test).map_err(|e| {
                            anyhow!(
                                "err:domain => domain:{:?}, e:{}",
                                domain_config_context.stream_config_context.domain,
                                e
                            )
                        })?;
                    }
                    let domain_index = Rc::new(
                        util::domain_index::DomainIndex::new(&index_map).map_err(|e| {
                            anyhow!("err:domain => index_map:{:?}, e:{}", index_map, e)
                        })?,
                    );
                    let sni = quic::util::sni(&value.listens)?;

                    let listen_server: Rc<Box<dyn Server>> =
                        Rc::new(Box::new(ssl_server::Server::new(
                            value.listen_addr.clone().unwrap(),
                            value.common.as_ref().unwrap().reuseport,
                            value.tcp_config.clone().unwrap(),
                            sni.clone(),
                        )?));

                    domain_config_listen_map.insert(
                        key.clone(),
                        DomainConfigListen {
                            common: value.common.clone().unwrap(),
                            stream: value.stream.clone().unwrap(),
                            listen_server,
                            domain_config_context_map,
                            domain_index,
                            sni: Some(sni),
                            server_type: value.server_type,
                        },
                    );
                }
                &DomainListenType::Quic => {
                    let mut domain_config_context_map = HashMap::new();
                    let mut index_map = HashMap::new();
                    let mut index = 0;
                    for domain_config_context in value.domain_config_contexts.iter() {
                        index += 1;
                        index_map.insert(
                            index,
                            (
                                domain_config_context
                                    .stream_config_context
                                    .domain
                                    .clone()
                                    .unwrap(),
                                index,
                            ),
                        );
                        domain_config_context_map.insert(index, domain_config_context.clone());

                        let mut index_map_test = HashMap::new();
                        index_map_test.insert(
                            index,
                            (
                                domain_config_context
                                    .stream_config_context
                                    .domain
                                    .clone()
                                    .unwrap(),
                                index,
                            ),
                        );
                        util::domain_index::DomainIndex::new(&index_map_test).map_err(|e| {
                            anyhow!(
                                "err:domain => domain:{:?}, e:{}",
                                domain_config_context.stream_config_context.domain,
                                e
                            )
                        })?;
                    }
                    let domain_index = Rc::new(
                        util::domain_index::DomainIndex::new(&index_map).map_err(|e| {
                            anyhow!("err:domain => index_map:{:?}, e:{}", index_map, e)
                        })?,
                    );
                    let sni = quic::util::sni(&value.listens)?;

                    let listen_server: Rc<Box<dyn Server>> =
                        Rc::new(Box::new(quic_server::Server::new(
                            value.listen_addr.clone().unwrap(),
                            value.common.as_ref().unwrap().reuseport,
                            value.quic_config.clone().unwrap(),
                            sni.clone(),
                            #[cfg(feature = "anyproxy-ebpf")]
                            ebpf_add_sock_hash.clone(),
                        )?));

                    domain_config_listen_map.insert(
                        key.clone(),
                        DomainConfigListen {
                            common: value.common.clone().unwrap(),
                            stream: value.stream.clone().unwrap(),
                            listen_server,
                            domain_config_context_map,
                            domain_index,
                            sni: Some(sni),
                            server_type: value.server_type,
                        },
                    );
                }
            }
        }

        return Ok(domain_config_listen_map);
    }

    pub fn merger(
        old_domain_config_listen: &DomainConfigListen,
        mut new_domain_config_listen: DomainConfigListen,
    ) -> Result<DomainConfigListen> {
        if old_domain_config_listen.sni.is_some() && new_domain_config_listen.sni.is_some() {
            let old_sni = old_domain_config_listen.sni.as_ref().unwrap();
            old_sni.take_from(new_domain_config_listen.sni.as_ref().unwrap());
            new_domain_config_listen.sni = Some(old_sni.clone());
        }
        Ok(new_domain_config_listen)
    }
}

#[async_trait(?Send)]
impl proxy::Config for DomainConfig {
    async fn parse(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        tunnel_clients: TunnelClients,
    ) -> Result<()> {
        if config._domain.is_none() {
            return Ok(());
        }

        let _ = self
            .parse_config(
                config,
                ups,
                tunnel_clients,
                #[cfg(feature = "anyproxy-ebpf")]
                None,
            )
            .await?;
        Ok(())
    }
}
