use super::UpstreamData;
use super::UpstreamDynamicDomainData;
use super::UpstreamHeartbeat;
use super::UpstreamHeartbeatData;
use crate::config::config_toml;
use crate::config::config_toml::UpstreamDispatch;
use crate::config::config_toml::{ProxyPass, _UpstreamConfig};
use crate::quic::connect as quic_connect;
use crate::quic::endpoints;
use crate::ssl::connect as ssl_connect;
use crate::stream::connect;
use crate::tcp::connect as tcp_connect;
use crate::tunnel::connect as tunnel_connect;
use crate::tunnel2::connect as tunnel2_connect;
use crate::util;
use crate::TunnelClients;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct UpstreamConfig {}

impl UpstreamConfig {
    pub fn new() -> Result<UpstreamConfig> {
        Ok(UpstreamConfig {})
    }

    pub async fn parse_config(
        &self,
        config: &config_toml::ConfigToml,
        tunnel_clients: TunnelClients,
    ) -> Result<(
        Rc<HashMap<String, config_toml::TcpConfig>>,
        Rc<HashMap<String, config_toml::QuicConfig>>,
        HashMap<String, Arc<Mutex<UpstreamData>>>,
    )> {
        let mut tcp_config_map = HashMap::new();
        let mut quic_config_map = HashMap::new();
        let mut endpoints_map = HashMap::new();
        let mut ups_data_map = HashMap::new();

        for tcp in config.tcp.iter() {
            if tcp_config_map.get(&tcp.tcp_name).is_some() {
                return Err(anyhow!("err:config.tcp.name={}", tcp.tcp_name));
            }
            tcp_config_map.insert(tcp.tcp_name.clone(), tcp.clone());
        }
        let common = &config.common;
        for quic in config.quic.iter() {
            if quic_config_map.get(&quic.quic_name).is_some() {
                return Err(anyhow!("err:config.tcp.name={}", quic.quic_name));
            }
            quic_config_map.insert(quic.quic_name.clone(), quic.clone());
            endpoints_map.insert(
                quic.quic_name.clone(),
                Arc::new(endpoints::Endpoints::new(quic, common.reuseport)?),
            );
        }
        let mut ups_configs = _UpstreamConfig {
            _server: Vec::new(),
        };
        if config._upstream.is_some() {
            ups_configs = config._upstream.clone().take().unwrap();
        }
        let mut upstream_name_map = HashMap::new();
        for ups_config in ups_configs._server.iter() {
            upstream_name_map.insert(ups_config.name.clone(), true);
        }
        let mut filter_upstream_map = HashMap::new();
        if config._port.is_some() {
            for port_server_config in config._port.as_ref().unwrap()._server.iter() {
                let upstream_name = port_server_config.proxy_pass_upstream.clone();
                if port_server_config.is_upstream {
                    if upstream_name_map.get(&upstream_name).is_none() {
                        return Err(anyhow!(
                            "err: upstream_name nil => upstream_name:{}",
                            upstream_name
                        ));
                    }
                    continue;
                }

                if filter_upstream_map.get(&upstream_name).is_some() {
                    log::info!("continue upstream_name:{}", upstream_name);
                    continue;
                }
                filter_upstream_map.insert(upstream_name.clone(), true);

                let ups_config = config_toml::UpstreamServerConfig {
                    name: upstream_name,
                    dispatch: config_toml::UpstreamDispatch::RoundRobin,
                    proxy_pass: vec![port_server_config.proxy_pass.clone()],
                };

                ups_configs._server.push(ups_config);
            }
        }
        if config._domain.is_some() {
            for domain_server_config in config._domain.as_ref().unwrap()._server.iter() {
                let upstream_name = domain_server_config.proxy_pass_upstream.clone();
                if domain_server_config.is_upstream {
                    if upstream_name_map.get(&upstream_name).is_none() {
                        return Err(anyhow!(
                            "err: upstream_name nil => upstream_name:{}",
                            upstream_name
                        ));
                    }
                    continue;
                }

                if filter_upstream_map.get(&upstream_name).is_some() {
                    continue;
                }
                filter_upstream_map.insert(upstream_name.clone(), true);

                let ups_config = config_toml::UpstreamServerConfig {
                    name: domain_server_config.proxy_pass_upstream.clone(),
                    dispatch: config_toml::UpstreamDispatch::RoundRobin,
                    proxy_pass: vec![domain_server_config.proxy_pass.clone()],
                };
                ups_configs._server.push(ups_config);
            }
        }

        let tcp_config_map = Rc::new(tcp_config_map);
        let quic_config_map = Rc::new(quic_config_map);
        let endpoints_map = Rc::new(endpoints_map);

        for ups_config in ups_configs._server.iter() {
            if ups_data_map.get(&ups_config.name).is_some() {
                return Err(anyhow!("err:ups_config.name={}", ups_config.name));
            }

            let mut ups_dynamic_domains = Vec::new();
            let mut ups_heartbeats = Vec::new();
            let mut ups_heartbeats_active = Vec::new();
            let mut ups_heartbeats_index: usize = 0;
            let mut ups_heartbeats_map = HashMap::new();

            let ups_heartbeat = if ups_config.dispatch == UpstreamDispatch::Fair {
                use crate::config::config_toml::default_heartbeat_fail;
                use crate::config::config_toml::default_heartbeat_interval;
                use crate::config::config_toml::default_heartbeat_timeout;
                Some(UpstreamHeartbeat {
                    interval: default_heartbeat_interval(),
                    timeout: default_heartbeat_timeout(),
                    fail: default_heartbeat_fail(),
                })
            } else {
                None
            };

            let is_weight = if ups_config.dispatch == UpstreamDispatch::Weight {
                true
            } else {
                false
            };

            for proxy_pass in ups_config.proxy_pass.iter() {
                let (address, dynamic_domain) = match &proxy_pass {
                    config_toml::ProxyPass::Tcp(tcp) => {
                        (tcp.address.clone(), tcp.dynamic_domain.clone())
                    }
                    config_toml::ProxyPass::Ssl(ssl) => {
                        (ssl.address.clone(), ssl.dynamic_domain.clone())
                    }
                    config_toml::ProxyPass::Quic(quic) => {
                        (quic.address.clone(), quic.dynamic_domain.clone())
                    }
                    config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                        config_toml::ProxyPassTunnel::Tcp(tcp) => {
                            (tcp.address.clone(), tcp.dynamic_domain.clone())
                        }
                        config_toml::ProxyPassTunnel::Ssl(ssl) => {
                            (ssl.address.clone(), ssl.dynamic_domain.clone())
                        }
                        config_toml::ProxyPassTunnel::Quic(quic) => {
                            (quic.address.clone(), quic.dynamic_domain.clone())
                        }
                    },
                    config_toml::ProxyPass::Tunnel2(tunnel) => match tunnel {
                        config_toml::ProxyPassTunnel2::Tcp(tcp) => {
                            (tcp.address.clone(), tcp.dynamic_domain.clone())
                        }
                        config_toml::ProxyPassTunnel2::Ssl(ssl) => {
                            (ssl.address.clone(), ssl.dynamic_domain.clone())
                        }
                        config_toml::ProxyPassTunnel2::Quic(quic) => {
                            (quic.address.clone(), quic.dynamic_domain.clone())
                        }
                    },
                    config_toml::ProxyPass::Upstream(upstream) => {
                        return Err(anyhow!("err:upstream.name={:?}", upstream))
                    }
                };
                let mut addrs =
                    util::util::lookup_hosts(tokio::time::Duration::from_secs(30), &address)
                        .await
                        .map_err(|e| anyhow!("err:lookup_host => address:{} e:{}", address, e))?;
                addrs.sort_by(|a, b| a.partial_cmp(b).unwrap());

                let domaon_index = ups_dynamic_domains.len();
                let ups_dynamic_domain = UpstreamDynamicDomainData {
                    index: domaon_index,
                    dynamic_domain,
                    proxy_pass: proxy_pass.clone(),
                    host: address.clone(),
                    addrs: addrs.clone(),
                    ups_heartbeat: ups_heartbeat.clone(),
                    is_weight,
                };
                ups_dynamic_domains.push(ups_dynamic_domain);

                for addr in addrs.iter() {
                    let ups_heartbeat = UpstreamConfig::heartbeat(
                        tcp_config_map.clone(),
                        quic_config_map.clone(),
                        endpoints_map.clone(),
                        tunnel_clients.clone(),
                        domaon_index,
                        ups_heartbeats_index,
                        addr,
                        proxy_pass,
                        address.clone(),
                        ups_heartbeat.clone(),
                        is_weight,
                    )?;
                    ups_heartbeats.push(ups_heartbeat.clone());
                    ups_heartbeats_active.push(ups_heartbeat.clone());
                    ups_heartbeats_map.insert(ups_heartbeats_index, ups_heartbeat);
                    ups_heartbeats_index += 1;
                }
            }

            let ups_data = Arc::new(Mutex::new(UpstreamData {
                is_heartbeat_disable: false,
                is_dynamic_domain_change: false,
                is_sort_heartbeats_active: false,
                ups_config: ups_config.clone(),
                ups_dynamic_domains,
                ups_heartbeats,
                ups_heartbeats_active,
                ups_heartbeats_map,
                ups_heartbeats_index,
                tcp_config_map: tcp_config_map.clone(),
                quic_config_map: quic_config_map.clone(),
                endpoints_map: endpoints_map.clone(),
                tunnel_clients: tunnel_clients.clone(),
                round_robin_index: 0,
            }));
            ups_data_map.insert(ups_config.name.clone(), ups_data);
        }

        Ok((tcp_config_map, quic_config_map, ups_data_map))
    }

    pub fn heartbeat(
        tcp_config_map: Rc<HashMap<String, config_toml::TcpConfig>>,
        quic_config_map: Rc<HashMap<String, config_toml::QuicConfig>>,
        endpoints_map: Rc<HashMap<String, Arc<endpoints::Endpoints>>>,
        tunnel_clients: TunnelClients,
        domain_index: usize,
        ups_heartbeats_index: usize,
        addr: &SocketAddr,
        proxy_pass: &ProxyPass,
        host: String,
        ups_heartbeat: Option<UpstreamHeartbeat>,
        is_weight: bool,
    ) -> Result<Rc<RefCell<UpstreamHeartbeatData>>> {
        let (heartbeat, connect, is_proxy_protocol_hello, weight): (
            Option<UpstreamHeartbeat>,
            Arc<Box<dyn connect::Connect>>,
            Option<bool>,
            Option<i64>,
        ) = match &proxy_pass {
            config_toml::ProxyPass::Tcp(tcp) => {
                let tcp_str = tcp.tcp.clone().take().unwrap();
                let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                if tcp_config.is_none() {
                    return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                }
                let connect = Box::new(tcp_connect::Connect::new(
                    host,
                    addr.clone(),
                    tcp_config.unwrap(),
                ));

                (
                    tcp.heartbeat.clone(),
                    Arc::new(connect),
                    tcp.is_proxy_protocol_hello,
                    tcp.weight,
                )
            }
            config_toml::ProxyPass::Ssl(ssl) => {
                let tcp_str = ssl.tcp.clone().take().unwrap();
                let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                if tcp_config.is_none() {
                    return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                }
                let connect = Box::new(ssl_connect::Connect::new(
                    host,
                    addr.clone(),
                    ssl.ssl_domain.clone(),
                    tcp_config.unwrap(),
                ));

                (
                    ssl.heartbeat.clone(),
                    Arc::new(connect),
                    ssl.is_proxy_protocol_hello,
                    ssl.weight,
                )
            }
            config_toml::ProxyPass::Quic(quic) => {
                let quic_str = quic.quic.clone().unwrap();
                let quic_config = quic_config_map.get(&quic_str).cloned();
                if quic_config.is_none() {
                    return Err(anyhow!("err:quic.quic={}", quic_str));
                }
                let endpoints = endpoints_map.get(&quic_str).cloned().unwrap();
                let connect = Box::new(quic_connect::Connect::new(
                    host,
                    addr.clone(),
                    quic.ssl_domain.clone(),
                    endpoints,
                    quic_config.unwrap(),
                ));
                (
                    quic.heartbeat.clone(),
                    Arc::new(connect),
                    quic.is_proxy_protocol_hello,
                    quic.weight,
                )
            }
            config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                config_toml::ProxyPassTunnel::Tcp(tcp) => {
                    let tcp_str = tcp.tcp.clone().take().unwrap();
                    let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                    if tcp_config.is_none() {
                        return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                    }
                    let connect = Box::new(tunnel_connect::Connect::new(
                        tunnel_clients.client.clone(),
                        Box::new(tunnel_connect::PeerStreamConnectTcp::new(
                            host,
                            addr.clone(),
                            tcp_config.unwrap(),
                            tcp.tunnel.max_stream_size,
                            tcp.tunnel.min_stream_cache_size,
                            tcp.tunnel.channel_size,
                        )),
                    ));
                    (
                        tcp.heartbeat.clone(),
                        Arc::new(connect),
                        tcp.is_proxy_protocol_hello,
                        tcp.weight,
                    )
                }
                config_toml::ProxyPassTunnel::Ssl(ssl) => {
                    let tcp_str = ssl.tcp.clone().take().unwrap();
                    let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                    if tcp_config.is_none() {
                        return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                    }
                    let connect = Box::new(tunnel_connect::Connect::new(
                        tunnel_clients.client.clone(),
                        Box::new(tunnel_connect::PeerStreamConnectSsl::new(
                            host,
                            addr.clone(),
                            ssl.ssl_domain.clone(),
                            tcp_config.unwrap(),
                            ssl.tunnel.max_stream_size,
                            ssl.tunnel.min_stream_cache_size,
                            ssl.tunnel.channel_size,
                        )),
                    ));
                    (
                        ssl.heartbeat.clone(),
                        Arc::new(connect),
                        ssl.is_proxy_protocol_hello,
                        ssl.weight,
                    )
                }
                config_toml::ProxyPassTunnel::Quic(quic) => {
                    let quic_str = quic.quic.clone().unwrap();
                    let quic_config = quic_config_map.get(&quic_str).cloned();
                    if quic_config.is_none() {
                        return Err(anyhow!("err:quic.quic={}", quic_str));
                    }
                    let endpoints = endpoints_map.get(&quic_str).cloned().unwrap();
                    let connect = Box::new(tunnel_connect::Connect::new(
                        tunnel_clients.client.clone(),
                        Box::new(tunnel_connect::PeerStreamConnectQuic::new(
                            host,
                            addr.clone(),
                            quic.ssl_domain.clone(),
                            endpoints,
                            quic.tunnel.max_stream_size,
                            quic.tunnel.min_stream_cache_size,
                            quic.tunnel.channel_size,
                            quic_config.unwrap(),
                        )),
                    ));
                    (
                        quic.heartbeat.clone(),
                        Arc::new(connect),
                        quic.is_proxy_protocol_hello,
                        quic.weight,
                    )
                }
            },
            config_toml::ProxyPass::Tunnel2(tunnel) => match tunnel {
                config_toml::ProxyPassTunnel2::Tcp(tcp) => {
                    let tcp_str = tcp.tcp.clone().take().unwrap();
                    let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                    if tcp_config.is_none() {
                        return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                    }
                    let connect = Box::new(tunnel2_connect::Connect::new(
                        tunnel_clients.client2.clone(),
                        Box::new(tunnel2_connect::PeerStreamConnectTcp::new(
                            host,
                            addr.clone(),
                            tcp_config.unwrap(),
                        )),
                    ));
                    (
                        tcp.heartbeat.clone(),
                        Arc::new(connect),
                        tcp.is_proxy_protocol_hello,
                        tcp.weight,
                    )
                }
                config_toml::ProxyPassTunnel2::Ssl(ssl) => {
                    let tcp_str = ssl.tcp.clone().take().unwrap();
                    let tcp_config = tcp_config_map.get(&tcp_str).cloned();
                    if tcp_config.is_none() {
                        return Err(anyhow!("err:tcp.tcp={}", tcp_str));
                    }
                    let connect = Box::new(tunnel2_connect::Connect::new(
                        tunnel_clients.client2.clone(),
                        Box::new(tunnel2_connect::PeerStreamConnectSsl::new(
                            host,
                            addr.clone(),
                            ssl.ssl_domain.clone(),
                            tcp_config.unwrap(),
                        )),
                    ));
                    (
                        ssl.heartbeat.clone(),
                        Arc::new(connect),
                        ssl.is_proxy_protocol_hello,
                        ssl.weight,
                    )
                }
                config_toml::ProxyPassTunnel2::Quic(quic) => {
                    let quic_str = quic.quic.clone().unwrap();
                    let quic_config = quic_config_map.get(&quic_str).cloned();
                    if quic_config.is_none() {
                        return Err(anyhow!("err:quic.quic={}", quic_str));
                    }
                    let endpoints = endpoints_map.get(&quic_str).cloned().unwrap();
                    let connect = Box::new(tunnel2_connect::Connect::new(
                        tunnel_clients.client2.clone(),
                        Box::new(tunnel2_connect::PeerStreamConnectQuic::new(
                            host,
                            addr.clone(),
                            quic.ssl_domain.clone(),
                            endpoints,
                            quic_config.unwrap(),
                        )),
                    ));
                    (
                        quic.heartbeat.clone(),
                        Arc::new(connect),
                        quic.is_proxy_protocol_hello,
                        quic.weight,
                    )
                }
            },
            config_toml::ProxyPass::Upstream(upstream) => {
                return Err(anyhow!("err:upstream.name={:?}", upstream))
            }
        };

        let heartbeat = if heartbeat.is_some() {
            heartbeat
        } else {
            ups_heartbeat
        };

        if is_weight && weight.is_none() {
            return Err(anyhow!("err:weight nil, proxy_pass={:?}", proxy_pass));
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
        let ups_heartbeat = Rc::new(RefCell::new(ups_heartbeat));
        Ok(ups_heartbeat)
    }

    // async fn parse(
    //     &self,
    //     config: &config_toml::ConfigToml,
    //     tunnel_clients: TunnelClients,
    // ) -> Result<()> {
    //     if config.upstream.is_none() {
    //         return Ok(());
    //     }
    //
    //     let _ = self
    //         .parse_config(config, tunnel_clients)
    //         .await
    //         .map_err(|e| anyhow!("err:parse_config => e:{}", e))?;
    //     Ok(())
    // }
}
