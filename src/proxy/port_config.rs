use super::proxy;
use super::stream_connect;
use super::stream_var;
use crate::config::config_toml;
use crate::config::config_toml::PortListen;
use crate::quic;
use crate::quic::connect as quic_connect;
use crate::quic::endpoints;
use crate::quic::server as quic_server;
use crate::stream::connect;
use crate::stream::server;
use crate::tcp::connect as tcp_connect;
use crate::tcp::server as tcp_server;
use crate::tunnel::connect as tunnel_connect;
use crate::tunnel2::connect as tunnel2_connect;
use crate::util;
use crate::util::var::Var;
use any_tunnel::client as tunnel_client;
use any_tunnel2::client as tunnel2_client;
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

pub struct PortConfigContext {
    pub common: config_toml::CommonConfig,
    pub tcp: config_toml::TcpConfig,
    pub quic: config_toml::QuicConfig,
    pub stream: config_toml::StreamConfig,
    pub access: Vec<config_toml::AccessConfig>,
    pub access_context: Vec<proxy::AccessContext>,
    pub domain: Option<String>,
    pub proxy_protocol: bool,
    //pub proxy_pass: config_toml::ProxyPass,
    pub connect: Rc<Box<dyn connect::Connect>>,
    pub endpoints: Arc<endpoints::Endpoints>,
    pub stream_var: Rc<stream_var::StreamVar>,
}

#[derive(Clone)]
pub struct PortConfigListen {
    //pub listen_config: ListenConfig,
    pub listen_server: Rc<Box<dyn server::Server>>,
    pub port_config_context: Rc<PortConfigContext>,
}

pub struct PortConfig {}

impl PortConfig {
    pub fn new() -> anyhow::Result<PortConfig> {
        Ok(PortConfig {})
    }
    pub fn tcp_key_from_addr(addr: &str) -> anyhow::Result<String> {
        Ok("tcp".to_string() + &util::util::addr(addr)?.port().to_string())
    }
    pub fn udp_key_from_addr(addr: &str) -> anyhow::Result<String> {
        Ok("udp".to_string() + &util::util::addr(addr)?.port().to_string())
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        tunnel_client: tunnel_client::Client,
        tunnel2_client: tunnel2_client::Client,
    ) -> anyhow::Result<HashMap<String, PortConfigListen>> {
        let mut access_map = HashMap::new();
        let mut port_config_listen_map = HashMap::new();

        let quic = config._port.as_ref().unwrap().quic.as_ref().unwrap();
        let common = &config.common;
        let endpoints = Arc::new(endpoints::Endpoints::new(quic, common.reuseport)?);

        for port_server_config in config._port.as_ref().unwrap()._server.iter() {
            let stream_var = stream_var::StreamVar::new();
            let stream_connect = stream_connect::StreamConnect::new(
                "tcp".to_string(),
                SocketAddr::from(([127, 0, 0, 1], 8080)),
                SocketAddr::from(([127, 0, 0, 1], 18080)),
            );
            let access_context = if port_server_config.access.is_some() {
                let mut access_context = Vec::new();
                for access in port_server_config.access.as_ref().unwrap() {
                    let ret: anyhow::Result<Var> = async {
                        let access_format_vars =
                            util::var::Var::new(&access.access_format, Some("-"))?;
                        let mut access_format_var = util::var::Var::copy(&access_format_vars)?;
                        access_format_var.for_each(|var| {
                            let var_name = util::var::Var::var_name(var);
                            let value = stream_var.find(var_name, &stream_connect)?;
                            Ok(value)
                        })?;
                        let _ = access_format_var.join()?;
                        Ok(access_format_vars)
                    }
                    .await;
                    let access_format_vars = ret.map_err(|e| {
                        anyhow::anyhow!(
                            "err:access_format => access_format:{}, e:{}",
                            access.access_format,
                            e
                        )
                    })?;

                    let ret: anyhow::Result<()> = async {
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
                            .map_err(|e| anyhow::anyhow!("err:path.canonicalize() => e:{}", e))?;
                        let path = canonicalize
                            .to_str()
                            .ok_or(anyhow::anyhow!("err:{}", access.access_log_file))?
                            .to_string();

                        let access_log_file = match access_map.get(&path).cloned() {
                            Some(access_log_file) => access_log_file,
                            None => {
                                let access_log_file = std::fs::OpenOptions::new()
                                    .append(true)
                                    .create(true)
                                    .open(&access.access_log_file)
                                    .map_err(|e| {
                                        anyhow::anyhow!(
                                            "err::open {} => e:{}",
                                            access.access_log_file,
                                            e
                                        )
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
                        anyhow::anyhow!(
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

            let (endpoints, quic) = if port_server_config.quic.is_some() {
                let quic = port_server_config.quic.as_ref().unwrap();
                let common = port_server_config.common.as_ref().unwrap();
                let endpoints = Arc::new(endpoints::Endpoints::new(quic, common.reuseport)?);
                (endpoints, quic)
            } else {
                (endpoints.clone(), quic)
            };

            let (address, connect): (String, Rc<Box<dyn connect::Connect>>) =
                match &port_server_config.proxy_pass {
                    config_toml::ProxyPass::Tcp(tcp) => (
                        tcp.address.clone(),
                        Rc::new(Box::new(tcp_connect::Connect::new(
                            tcp.address.clone(),
                            tokio::time::Duration::from_secs(
                                port_server_config
                                    .stream
                                    .as_ref()
                                    .unwrap()
                                    .stream_connect_timeout,
                            ),
                            port_server_config.tcp.clone().take().unwrap(),
                        )?)),
                    ),
                    config_toml::ProxyPass::Quic(quic) => (
                        quic.address.clone(),
                        Rc::new(Box::new(quic_connect::Connect::new(
                            quic.address.clone(),
                            quic.ssl_domain.clone(),
                            tokio::time::Duration::from_secs(
                                port_server_config
                                    .stream
                                    .as_ref()
                                    .unwrap()
                                    .stream_connect_timeout,
                            ),
                            endpoints.clone(),
                        )?)),
                    ),
                    config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                        config_toml::ProxyPassTunnel::Tcp(tcp) => (
                            tcp.address.clone(),
                            Rc::new(Box::new(tunnel_connect::Connect::new(
                                tunnel_client.clone(),
                                Box::new(tunnel_connect::PeerStreamConnectTcp::new(
                                    tcp.address.clone(),
                                    tokio::time::Duration::from_secs(
                                        port_server_config
                                            .stream
                                            .as_ref()
                                            .unwrap()
                                            .stream_connect_timeout,
                                    ),
                                    port_server_config.tcp.clone().take().unwrap(),
                                    tcp.tunnel_max_connect,
                                )),
                            )?)),
                        ),
                        config_toml::ProxyPassTunnel::Quic(quic) => (
                            quic.address.clone(),
                            Rc::new(Box::new(tunnel_connect::Connect::new(
                                tunnel_client.clone(),
                                Box::new(tunnel_connect::PeerStreamConnectQuic::new(
                                    quic.address.clone(),
                                    quic.ssl_domain.clone(),
                                    tokio::time::Duration::from_secs(
                                        port_server_config
                                            .stream
                                            .as_ref()
                                            .unwrap()
                                            .stream_connect_timeout,
                                    ),
                                    endpoints.clone(),
                                    quic.tunnel_max_connect,
                                )),
                            )?)),
                        ),
                        config_toml::ProxyPassTunnel::Upstrem(upstream) => {
                            return Err(anyhow::anyhow!("err:not support upstream{}", upstream));
                        }
                    },
                    config_toml::ProxyPass::Tunnel2(tunnel) => match tunnel {
                        config_toml::ProxyPassTunnel2::Tcp(tcp) => (
                            tcp.address.clone(),
                            Rc::new(Box::new(tunnel2_connect::Connect::new(
                                tunnel2_client.clone(),
                                Box::new(tunnel2_connect::PeerStreamConnectTcp::new(
                                    tcp.address.clone(),
                                    tokio::time::Duration::from_secs(
                                        port_server_config
                                            .stream
                                            .as_ref()
                                            .unwrap()
                                            .stream_connect_timeout,
                                    ),
                                    port_server_config.tcp.clone().take().unwrap(),
                                )),
                            )?)),
                        ),
                        config_toml::ProxyPassTunnel2::Quic(quic) => (
                            quic.address.clone(),
                            Rc::new(Box::new(tunnel2_connect::Connect::new(
                                tunnel2_client.clone(),
                                Box::new(tunnel2_connect::PeerStreamConnectQuic::new(
                                    quic.address.clone(),
                                    quic.ssl_domain.clone(),
                                    tokio::time::Duration::from_secs(
                                        port_server_config
                                            .stream
                                            .as_ref()
                                            .unwrap()
                                            .stream_connect_timeout,
                                    ),
                                    endpoints.clone(),
                                )),
                            )?)),
                        ),
                        config_toml::ProxyPassTunnel2::Upstrem(upstream) => {
                            return Err(anyhow::anyhow!("err:not support upstream{}", upstream));
                        }
                    },
                    config_toml::ProxyPass::Upstrem(upstream) => {
                        return Err(anyhow::anyhow!("err:not support upstream{}", upstream));
                    }
                };
            let _ = util::util::lookup_host(tokio::time::Duration::from_secs(10), &address)
                .await
                .map_err(|e| anyhow::anyhow!("err:lookup_host => address:{} e:{}", address, e))?;

            let port_config_context = Rc::new(PortConfigContext {
                common: port_server_config.common.clone().take().unwrap(),
                tcp: port_server_config.tcp.clone().take().unwrap(),
                quic: quic.clone(),
                stream: port_server_config.stream.clone().take().unwrap(),
                access: port_server_config.access.clone().take().unwrap(),
                access_context: access_context.unwrap(),
                domain: port_server_config.domain.clone(),
                proxy_protocol: port_server_config.proxy_protocol,
                //proxy_pass: port_server_config.proxy_pass.clone(),
                connect: connect.clone(),
                endpoints,
                stream_var: Rc::new(stream_var::StreamVar::new()),
            });

            for listen in port_server_config.listen.iter() {
                match listen {
                    PortListen::Tcp(listen) => {
                        let ret: anyhow::Result<()> = async {
                            let addrs = util::util::str_addrs(&listen.address)?;
                            let sock_addrs = util::util::addrs(&addrs)?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let key = PortConfig::tcp_key_from_addr(addr)?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow::anyhow!(
                                        "err:addr is exist => addr:{}",
                                        addr
                                    ));
                                }

                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(tcp_server::Server::new(
                                        sock_addrs[index].clone(),
                                        port_config_context.common.reuseport,
                                        port_server_config.tcp.clone().take().unwrap(),
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
                            anyhow::anyhow!("err:address => address:{}, e:{}", listen.address, e)
                        })?;
                    }
                    PortListen::Quic(listen) => {
                        let ret: anyhow::Result<()> = async {
                            let addrs = util::util::str_addrs(&listen.address)?;
                            let sock_addrs = util::util::addrs(&addrs)?;
                            for (index, addr) in addrs.iter().enumerate() {
                                let key = PortConfig::udp_key_from_addr(addr)?;
                                if port_config_listen_map.get(&key).is_some() {
                                    return Err(anyhow::anyhow!(
                                        "err:addr is exist => addr:{}",
                                        addr
                                    ));
                                }

                                let listen = config_toml::Listen {
                                    address: addr.clone(),
                                    ssl: Some(listen.ssl.clone()),
                                };

                                let sni = quic::util::sni(&vec![listen.clone()])?;

                                let listen_server: Rc<Box<dyn server::Server>> =
                                    Rc::new(Box::new(quic_server::Server::new(
                                        sock_addrs[index].clone(),
                                        port_config_context.common.reuseport,
                                        quic.clone(),
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
                            anyhow::anyhow!("err:address => address:{}, e:{}", listen.address, e)
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
    ) -> anyhow::Result<PortConfigListen> {
        let old_sni = old_port_config_listen.listen_server.sni();
        let new_sni = new_port_config_listen.listen_server.sni();
        if old_sni.is_none() || new_sni.is_none() {
            return Err(anyhow::anyhow!("err:merget sni"))?;
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
        tunnel_client: tunnel_client::Client,
        tunnel2_client: tunnel2_client::Client,
    ) -> anyhow::Result<()> {
        if config._port.is_none() {
            return Ok(());
        }

        let _ = self
            .parse_config(config, tunnel_client, tunnel2_client)
            .await?;
        Ok(())
    }
}
