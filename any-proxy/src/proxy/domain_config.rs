use super::proxy;
use super::stream_info;
use super::stream_var;
use super::StreamConfigContext;
use crate::config::config_toml;
use crate::config::config_toml::DomainListen;
use crate::config::config_toml::Listen;
use crate::config::config_toml::SSL;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::quic;
use crate::quic::server as quic_server;
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
use std::path::Path;
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
    quic_sni: Option<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>>,
}

#[derive(Clone)]
pub struct DomainConfigListenMerge {
    pub key_type: DomainConfigKeyType,
    pub common: Option<config_toml::CommonConfig>,
    pub stream: Option<config_toml::StreamConfig>,
    pub tcp_config: Option<config_toml::TcpConfig>,
    pub quic_config: Option<config_toml::QuicConfig>,
    pub listen_addr: Option<SocketAddr>,
    pub listens: Vec<Listen>,
    pub domain_config_contexts: Vec<Rc<DomainConfigContext>>,
}

#[derive(Clone)]
pub enum DomainConfigKeyType {
    Tcp = 0,
    Udp = 1,
    Quic = 2,
}

pub struct DomainConfig {}

impl DomainConfig {
    pub fn new() -> Result<DomainConfig> {
        Ok(DomainConfig {})
    }
    pub fn tcp_key_from_addr(addr: &str) -> Result<String> {
        Ok("tcp".to_string() + &util::util::addr(addr)?.port().to_string())
    }
    pub fn udp_key_from_addr(addr: &str) -> Result<String> {
        Ok("udp".to_string() + &util::util::addr(addr)?.port().to_string())
    }
    pub fn quic_key_from_addr(addr: &str) -> Result<String> {
        Ok("quic".to_string() + &util::util::addr(addr)?.port().to_string())
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        _tunnel_clients: TunnelClients,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Option<Arc<any_ebpf::AddSockHash>>,
    ) -> Result<HashMap<String, DomainConfigListen>> {
        let mut access_map = HashMap::new();
        let mut key_map = HashMap::new();
        let mut domain_config_listen_map = HashMap::new();
        let mut domain_config_listen_merge_map = HashMap::new();

        let quic = config._domain.as_ref().unwrap().quic.as_ref().unwrap();
        let quic_config = ups.quic_config(quic);
        if quic_config.is_none() {
            return Err(anyhow!("err: quic_str => quic:{}", quic));
        }

        for domain_server_config in config._domain.as_ref().unwrap()._server.iter() {
            let stream_var = stream_var::StreamVar::new();
            let stream_info_test = stream_info::StreamInfo::new(
                "tcp".to_string(),
                SocketAddr::from(([127, 0, 0, 1], 8080)),
                SocketAddr::from(([127, 0, 0, 1], 18080)),
                false,
            );
            let access_context = if domain_server_config.access.is_some() {
                let mut access_context = Vec::new();
                for access in domain_server_config.access.as_ref().unwrap() {
                    let ret: Result<util::var::Var> = async {
                        let access_format_vars =
                            util::var::Var::new(&access.access_format, Some("-"))
                                .map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
                        let mut access_format_vars_test = util::var::Var::copy(&access_format_vars)
                            .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
                        access_format_vars_test.for_each(|var| {
                            let var_name = util::var::Var::var_name(var);
                            let value = stream_var
                                .find(var_name, &stream_info_test)
                                .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                            Ok(value)
                        })?;
                        let _ = access_format_vars_test
                            .join()
                            .map_err(|e| anyhow!("err:access_format_vars_test.join => e:{}", e))?;
                        Ok(access_format_vars)
                    }
                    .await;
                    let access_format_vars = ret.map_err(|e| {
                        anyhow!(
                            "err:access_format => access_format:{}, e:{}",
                            access.access_format,
                            e
                        )
                    })?;

                    let ret: Result<()> = async {
                        {
                            //就是创建下文件 啥都不干
                            let _ = std::fs::OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(&access.access_log_file);
                        }
                        let path = Path::new(&access.access_log_file);
                        let canonicalize = path
                            .canonicalize()
                            .map_err(|e| anyhow!("err:path.canonicalize() => e:{}", e))?;
                        let path = canonicalize
                            .to_str()
                            .ok_or(anyhow!("err:{}", access.access_log_file))?
                            .to_string();
                        let access_log_file = match access_map.get(&path).cloned() {
                            Some(access_log_file) => access_log_file,
                            None => {
                                let access_log_file = std::fs::OpenOptions::new()
                                    .append(true)
                                    .create(true)
                                    .open(&access.access_log_file)
                                    .map_err(|e| {
                                        anyhow!("err::open {} => e:{}", access.access_log_file, e)
                                    })?;
                                let access_log_file = std::sync::Arc::new(access_log_file);
                                access_map.insert(path, access_log_file.clone());
                                access_log_file
                            }
                        };

                        access_context.push(proxy::AccessContext {
                            access_format_vars,
                            access_log_file,
                        });
                        Ok(())
                    }
                    .await;
                    ret.map_err(|e| {
                        anyhow!(
                            "err:access_log_file => access_log_file:{}, e:{}",
                            access.access_log_file,
                            e
                        )
                    })?;
                }
                Some(access_context)
            } else {
                None
            };
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

            let quic_config = ups.quic_config(&quic);
            if quic_config.is_none() {
                return Err(anyhow!("err:ups.quic => quic:{}", quic));
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
            });
            let domain_config_context = Rc::new(DomainConfigContext {
                stream_config_context: stream_config_context.clone(),
            });

            for listen in domain_server_config.listen.as_ref().unwrap().iter() {
                match listen {
                    DomainListen::Tcp(listen) => {
                        let ret: Result<()> = async {
                            let addrs = util::util::str_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::str_addrs => e:{}", e))?;
                            let sock_addrs = util::util::addrs(&addrs)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let key = DomainConfig::tcp_key_from_addr(addr).map_err(|e| {
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
                                            key_type: DomainConfigKeyType::Tcp,
                                            listen_addr: None,
                                            listens: Vec::new(),
                                            domain_config_contexts: Vec::new(),
                                        },
                                    );
                                }

                                let mut values =
                                    domain_config_listen_merge_map.get_mut(&key).unwrap();
                                let listen = config_toml::Listen {
                                    address: addr.clone(),
                                    ssl: None,
                                };

                                values.common = Some(stream_config_context.common.clone());
                                values.stream = Some(stream_config_context.stream.clone());
                                if values.tcp_config.is_none() {
                                    values.tcp_config = Some(stream_config_context.tcp.clone());
                                    values.quic_config = Some(stream_config_context.quic.clone());
                                }
                                values.listens.push(listen);
                                values.listen_addr = Some(sock_addrs[index]);
                                values
                                    .domain_config_contexts
                                    .push(domain_config_context.clone());
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
                            let addrs = util::util::str_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::str_addrs => e:{}", e))?;
                            let sock_addrs = util::util::addrs(&addrs)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let udp_key =
                                    DomainConfig::udp_key_from_addr(addr).map_err(|e| {
                                        anyhow!("err:DomainConfig::udp_key_from_addr => e:{}", e)
                                    })?;
                                let quic_key =
                                    DomainConfig::quic_key_from_addr(addr).map_err(|e| {
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
                                            key_type: DomainConfigKeyType::Quic,
                                            listen_addr: None,
                                            listens: Vec::new(),
                                            domain_config_contexts: Vec::new(),
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
                                    address: addr.clone(),
                                    ssl: Some(ssl),
                                };

                                values.common = Some(stream_config_context.common.clone());
                                values.stream = Some(stream_config_context.stream.clone());
                                if values.tcp_config.is_none() {
                                    values.tcp_config = Some(stream_config_context.tcp.clone());
                                    values.quic_config = Some(stream_config_context.quic.clone());
                                }
                                values.listen_addr = Some(sock_addrs[index]);
                                values.listens.push(listen);
                                values
                                    .domain_config_contexts
                                    .push(domain_config_context.clone());
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
            match &value.key_type {
                &DomainConfigKeyType::Tcp => {
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
                            quic_sni: None,
                        },
                    );
                }
                &DomainConfigKeyType::Udp => {
                    continue;
                }
                &DomainConfigKeyType::Quic => {
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
                    let sni = std::sync::Arc::new(quic::util::sni(&value.listens)?);

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
                            quic_sni: Some(sni),
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
        if old_domain_config_listen.quic_sni.is_some()
            && new_domain_config_listen.quic_sni.is_some()
        {
            let old_sni = old_domain_config_listen.quic_sni.as_ref().unwrap();
            old_sni.take_from(new_domain_config_listen.quic_sni.as_ref().unwrap());
            new_domain_config_listen.quic_sni = Some(old_sni.clone());
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
