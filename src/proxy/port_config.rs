use super::proxy;
use super::stream_info;
use super::stream_var;
use super::StreamConfigContext;
use crate::config::config_toml;
use crate::config::config_toml::PortListen;
use crate::quic;
use crate::quic::server as quic_server;
use crate::stream::server;
use crate::tcp::server as tcp_server;
use crate::upstream::upstream;
use crate::util;
use crate::util::var::Var;
use crate::TunnelClients;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;

pub struct PortConfigContext {
    pub stream_config_context: Rc<StreamConfigContext>,
}

#[derive(Clone)]
pub struct PortConfigListen {
    //pub listen_config: ListenConfig,
    pub listen_server: Rc<Box<dyn server::Server>>,
    pub port_config_context: Rc<PortConfigContext>,
}

pub struct PortConfig {}

impl PortConfig {
    pub fn new() -> Result<PortConfig> {
        Ok(PortConfig {})
    }
    pub fn tcp_key_from_addr(addr: &str) -> Result<String> {
        Ok("tcp".to_string() + &util::util::addr(addr)?.port().to_string())
    }
    pub fn udp_key_from_addr(addr: &str) -> Result<String> {
        Ok("udp".to_string() + &util::util::addr(addr)?.port().to_string())
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        _tunnel_clients: TunnelClients,
    ) -> Result<HashMap<String, PortConfigListen>> {
        let mut access_map = HashMap::new();
        let mut port_config_listen_map = HashMap::new();

        let quic = config._port.as_ref().unwrap().quic.as_ref().unwrap();

        for port_server_config in config._port.as_ref().unwrap()._server.iter() {
            let stream_var = stream_var::StreamVar::new();
            let stream_info_test = stream_info::StreamInfo::new(
                "tcp".to_string(),
                SocketAddr::from(([127, 0, 0, 1], 8080)),
                SocketAddr::from(([127, 0, 0, 1], 18080)),
            );
            let access_context = if port_server_config.access.is_some() {
                let mut access_context = Vec::new();
                for access in port_server_config.access.as_ref().unwrap() {
                    let ret: Result<Var> = async {
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
                            .map_err(|e| anyhow!("err:ccess_format_var.join => e:{}", e))?;
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

            let quic_config = ups.quic_config(&quic);
            if quic_config.is_none() {
                return Err(anyhow!("err:ups.quic => quic:{}", quic));
            }
            let quic_config = quic_config.unwrap();

            let stream_config_context = Rc::new(StreamConfigContext {
                common: port_server_config.common.clone().take().unwrap(),
                tcp: tcp_config.clone(),
                quic: quic_config.clone(),
                stream: port_server_config.stream.clone().take().unwrap(),
                rate: port_server_config.rate.clone().take().unwrap(),
                tmp_file: port_server_config.tmp_file.clone().take().unwrap(),
                fast: port_server_config.fast.clone().take().unwrap(),
                access: port_server_config.access.clone().take().unwrap(),
                access_context: access_context.unwrap(),
                domain: port_server_config.domain.clone(),
                proxy_protocol: port_server_config.proxy_protocol,
                //proxy_pass: port_server_config.proxy_pass.clone(),
                //connect: connect.clone(),
                //endpoints,
                ups_data: upstream_data.unwrap(),
                stream_var: Rc::new(stream_var::StreamVar::new()),
            });

            let port_config_context = Rc::new(PortConfigContext {
                stream_config_context: stream_config_context.clone(),
            });

            for listen in port_server_config.listen.iter() {
                match listen {
                    PortListen::Tcp(listen) => {
                        let ret: Result<()> = async {
                            let addrs = util::util::str_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::str_addrs => e:{}", e))?;
                            let sock_addrs = util::util::addrs(&addrs)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let key = PortConfig::tcp_key_from_addr(addr).map_err(|e| {
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
                                        sock_addrs[index].clone(),
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
                    PortListen::Quic(listen) => {
                        let ret: Result<()> = async {
                            let addrs = util::util::str_addrs(&listen.address)
                                .map_err(|e| anyhow!("err:util::str_addrs => e:{}", e))?;
                            let sock_addrs = util::util::addrs(&addrs)
                                .map_err(|e| anyhow!("err:util::addrs => e:{}", e))?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let key = PortConfig::udp_key_from_addr(addr).map_err(|e| {
                                    anyhow!("err:PortConfig::udp_key_from_addr => e:{}", e)
                                })?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow!("err:addr is exist => addr:{}", addr));
                                }

                                let listen = config_toml::Listen {
                                    address: addr.clone(),
                                    ssl: Some(listen.ssl.clone()),
                                };

                                let sni = quic::util::sni(&vec![listen.clone()])
                                    .map_err(|e| anyhow!("err:util::sni => e:{}", e))?;

                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(quic_server::Server::new(
                                        sock_addrs[index].clone(),
                                        stream_config_context.common.reuseport,
                                        quic_config.clone(),
                                        std::sync::Arc::new(sni),
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
            .parse_config(config, ups, tunnel_clients)
            .await
            .map_err(|e| anyhow!("err:parse_config => e:{}", e))?;
        Ok(())
    }
}
