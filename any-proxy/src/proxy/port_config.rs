use super::access_log::AccessLog;
use super::proxy;
use super::stream_var;
use super::StreamConfigContext;
use crate::config::config_toml;
use crate::config::config_toml::PortListen;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::quic;
use crate::quic::server as quic_server;
use crate::ssl::server as ssl_server;
use crate::stream::server;
use crate::tcp::server as tcp_server;
use crate::upstream::upstream;
use crate::util;
use crate::TunnelClients;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(feature = "anyproxy-ebpf")]
use std::sync::Arc;

pub struct PortConfigContext {
    pub stream_config_context: Rc<StreamConfigContext>,
}

#[derive(Clone)]
pub struct PortConfigListen {
    pub listen_server: Rc<Box<dyn server::Server>>,
    pub port_config_context: Rc<PortConfigContext>,
}

pub struct PortConfig {}

impl PortConfig {
    pub fn new() -> Result<PortConfig> {
        Ok(PortConfig {})
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        _tunnel_clients: TunnelClients,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Option<Arc<any_ebpf::AddSockHash>>,
    ) -> Result<HashMap<String, PortConfigListen>> {
        let mut port_config_listen_map = HashMap::new();

        for port_server_config in config._port.as_ref().unwrap()._server.iter() {
            let access_context =
                AccessLog::parse_config_access_log(&port_server_config.access).await?;

            let upstream_data = ups.upstream_data(&port_server_config.proxy_pass_upstream);
            if upstream_data.is_none() {
                return Err(anyhow!(
                    "err:ups.upstream_data => proxy_pass_upstream:{}",
                    port_server_config.proxy_pass_upstream
                ));
            }

            let tcp_str = port_server_config.tcp.clone().take().unwrap();

            let tcp_config = ups.tcp_config(&tcp_str);
            if tcp_config.is_none() {
                return Err(anyhow!("err:ups.tcp_str => tcp_str:{}", tcp_str));
            }
            let tcp_config = tcp_config.unwrap();

            let quic_str = port_server_config.quic.clone().take().unwrap();
            let quic_config = ups.quic_config(&quic_str);
            if quic_config.is_none() {
                return Err(anyhow!("err:ups.quic => quic_str:{}", quic_str));
            }
            let quic_config = quic_config.unwrap();

            let stream_config_context = Rc::new(StreamConfigContext {
                common: port_server_config.common.clone().take().unwrap(),
                tcp: tcp_config.clone(),
                quic: quic_config.clone(),
                stream: port_server_config.stream.clone().take().unwrap(),
                rate: port_server_config.rate.clone().take().unwrap(),
                tmp_file: port_server_config.tmp_file.clone().take().unwrap(),
                fast_conf: port_server_config.fast_conf.clone().take().unwrap(),
                access: port_server_config.access.clone().take().unwrap(),
                access_context: access_context.unwrap(),
                domain: port_server_config.domain.clone(),
                is_proxy_protocol_hello: port_server_config.is_proxy_protocol_hello,
                ups_data: upstream_data.unwrap(),
                stream_var: Rc::new(stream_var::StreamVar::new()),
                server: None,
            });

            let port_config_context = Rc::new(PortConfigContext {
                stream_config_context: stream_config_context.clone(),
            });

            for listen in port_server_config.listen.iter() {
                match listen {
                    PortListen::Tcp(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let key = util::util::tcp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:PortConfig::tcp_key_from_addr => e:{}", e)
                                })?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow!("err:addr is exist => addr:{}", addr));
                                }
                                let tcp_str = port_server_config.tcp.clone().take().unwrap();
                                let tcp_config = ups.tcp_config(&tcp_str);
                                if tcp_config.is_none() {
                                    return Err(anyhow!("err:tcp_str nil => tcp_str:{}", tcp_str));
                                }
                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(tcp_server::Server::new(
                                        addr.clone(),
                                        stream_config_context.common.reuseport,
                                        tcp_config.unwrap(),
                                    )?));
                                port_config_listen_map.insert(
                                    key,
                                    PortConfigListen {
                                        listen_server,
                                        port_config_context: port_config_context.clone(),
                                    },
                                );
                            }
                            Ok(())
                        }
                        .await;
                        ret.map_err(|e| {
                            anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                    PortListen::Ssl(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let key = util::util::tcp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:PortConfig::tcp_key_from_addr => e:{}", e)
                                })?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow!("err:addr is exist => addr:{}", addr));
                                }
                                let tcp_str = port_server_config.tcp.clone().take().unwrap();
                                let tcp_config = ups.tcp_config(&tcp_str);
                                if tcp_config.is_none() {
                                    return Err(anyhow!("err:tcp_str nil => tcp_str:{}", tcp_str));
                                }

                                let listen = config_toml::Listen {
                                    address: addr.to_string(),
                                    ssl: Some(listen.ssl.clone()),
                                };

                                let sni = quic::util::sni(&vec![listen.clone()])
                                    .map_err(|e| anyhow!("err:util::sni => e:{}", e))?;

                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(ssl_server::Server::new(
                                        addr.clone(),
                                        stream_config_context.common.reuseport,
                                        tcp_config.unwrap(),
                                        sni,
                                    )?));
                                port_config_listen_map.insert(
                                    key,
                                    PortConfigListen {
                                        listen_server,
                                        port_config_context: port_config_context.clone(),
                                    },
                                );
                            }
                            Ok(())
                        }
                        .await;
                        ret.map_err(|e| {
                            anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                    PortListen::Quic(listen) => {
                        let ret: Result<()> = async {
                            let sock_addrs = util::util::str_to_socket_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for addr in sock_addrs.iter() {
                                let key = util::util::udp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:PortConfig::udp_key_from_addr => e:{}", e)
                                })?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow!("err:addr is exist => addr:{}", addr));
                                }

                                let listen = config_toml::Listen {
                                    address: addr.to_string(),
                                    ssl: Some(listen.ssl.clone()),
                                };

                                let sni = quic::util::sni(&vec![listen.clone()])
                                    .map_err(|e| anyhow!("err:util::sni => e:{}", e))?;

                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(quic_server::Server::new(
                                        addr.clone(),
                                        stream_config_context.common.reuseport,
                                        quic_config.clone(),
                                        sni,
                                        #[cfg(feature = "anyproxy-ebpf")]
                                        ebpf_add_sock_hash.clone(),
                                    )?));

                                port_config_listen_map.insert(
                                    key,
                                    PortConfigListen {
                                        listen_server,
                                        port_config_context: port_config_context.clone(),
                                    },
                                );
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

        return Ok(port_config_listen_map);
    }

    pub fn merger(
        old_port_config_listen: &PortConfigListen,
        new_port_config_listen: PortConfigListen,
    ) -> Result<PortConfigListen> {
        let old_sni = old_port_config_listen.listen_server.sni();
        let new_sni = new_port_config_listen.listen_server.sni();
        if old_sni.is_none() || new_sni.is_none() {
            return Err(anyhow!("err:merget sni"))?;
        }
        old_sni
            .as_ref()
            .unwrap()
            .take_from(new_sni.as_ref().unwrap());
        new_port_config_listen
            .listen_server
            .set_sni(old_sni.unwrap());
        Ok(new_port_config_listen)
    }
}

#[async_trait(?Send)]
impl proxy::Config for PortConfig {
    async fn parse(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        tunnel_clients: TunnelClients,
    ) -> Result<()> {
        if config._port.is_none() {
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
            .await
            .map_err(|e| anyhow!("err:parse_config => e:{}", e))?;
        Ok(())
    }
}
