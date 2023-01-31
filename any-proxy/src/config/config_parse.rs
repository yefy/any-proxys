use super::config_toml;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;

pub struct ConfigVar<'a> {
    pub protocol: &'a str,
    pub server: &'a str,
}

lazy_static! {
    pub static ref CONFIG_VARS: Vec<ConfigVar<'static>> = vec![
        ConfigVar {
            protocol: "_port",
            server: "_server",
        },
        ConfigVar {
            protocol: "_domain",
            server: "_server",
        },
        ConfigVar {
            protocol: "_upstream",
            server: "_server",
        },
    ];
}

pub fn check(mut config: config_toml::ConfigToml) -> Result<config_toml::ConfigToml> {
    let core_ids = core_affinity::get_core_ids().unwrap();
    #[cfg(unix)]
    {
        if config.common.worker_threads == 0 {
            config.common.worker_threads = core_ids.len();
        }
        if config.common.worker_threads == 0 {
            config.common.worker_threads = 1;
        }

        if !config.common.reuseport {
            config.common.reuseport = true;
            log::info!("linux reuseport set true")
        }
    };

    #[cfg(windows)]
    {
        if config.common.reuseport {
            config.common.reuseport = false;
            log::info!("windows not support reuseport")
        }
        if config.common.worker_threads != 1 {
            config.common.worker_threads = 1;
            log::info!("windows worker_threads set 1")
        }
    };

    if config.tunnel2.tunnel2_worker_thread == 0 {
        config.tunnel2.tunnel2_worker_thread = core_ids.len();
    }
    if config.tunnel2.tunnel2_worker_thread == 0 {
        config.tunnel2.tunnel2_worker_thread = 1;
    }

    if config.common.debug_is_print_config {
        log::info!("config = {:#?}", config);
    }

    Ok(config)
}

pub fn merger(mut config: config_toml::ConfigToml) -> Result<config_toml::ConfigToml> {
    if config.tcp.len() <= 0 {
        return Err(anyhow!("config.tcp nil"));
    }

    if config.quic.len() <= 0 {
        return Err(anyhow!("config.quic nil"));
    }

    let mut tcp_server_func = || -> Result<()> {
        if config._port.is_none() {
            return Ok(());
        }

        let _port = config._port.as_mut().unwrap();
        if _port.tcp.is_none() {
            _port.tcp = Some(config.tcp[0].tcp_name.clone());
        }

        if _port.quic.is_none() {
            _port.quic = Some(config.quic[0].quic_name.clone());
        }

        if _port.tunnel2.is_none() {
            _port.tunnel2 = Some(config.tunnel2.clone());
        }

        if _port.stream.is_none() {
            _port.stream = Some(config.stream.clone());
        }

        if _port.rate.is_none() {
            _port.rate = Some(config.rate.clone());
        }

        if _port.tmp_file.is_none() {
            _port.tmp_file = Some(config.tmp_file.clone());
        }

        if _port.fast_conf.is_none() {
            _port.fast_conf = Some(config.fast_conf.clone());
        }

        for v in _port._server.iter_mut() {
            v.common = Some(config.common.clone());
            if v.tcp.is_none() {
                v.tcp = Some(_port.tcp.clone().unwrap())
            }

            if v.quic.is_none() {
                v.quic = Some(_port.quic.clone().unwrap())
            }

            if v.tunnel2.is_none() {
                v.tunnel2 = Some(_port.tunnel2.clone().unwrap())
            }

            if v.stream.is_none() {
                v.stream = Some(_port.stream.clone().unwrap())
            }

            if v.rate.is_none() {
                v.rate = Some(_port.rate.clone().unwrap())
            }

            if v.tmp_file.is_none() {
                v.tmp_file = Some(_port.tmp_file.clone().unwrap())
            }

            if v.fast_conf.is_none() {
                v.fast_conf = Some(_port.fast_conf.clone().unwrap())
            }

            if v.access.is_none() {
                v.access = Some(_port.access.clone())
            }

            let upstream_name = match &mut v.proxy_pass {
                config_toml::ProxyPass::Tcp(tcp) => {
                    if tcp.tcp.is_none() {
                        tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                    }
                    let upstream_name = "tcp_".to_string()
                        + &tcp.address.clone()
                        + "_"
                        + &tcp.tcp.clone().take().unwrap();
                    upstream_name
                }
                config_toml::ProxyPass::Quic(quic) => {
                    if quic.quic.is_none() {
                        quic.quic = Some(config.quic[0].quic_name.clone())
                    }
                    let upstream_name = "quic_".to_string()
                        + &quic.address.clone()
                        + "_"
                        + &quic.quic.clone().take().unwrap();
                    upstream_name
                }
                config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                    config_toml::ProxyPassTunnel::Tcp(tcp) => {
                        if tcp.tcp.is_none() {
                            tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                        }
                        let upstream_name = "tcp_tunnel_".to_string()
                            + &tcp.address.clone()
                            + "_"
                            + &tcp.tcp.clone().take().unwrap();
                        upstream_name
                    }
                    config_toml::ProxyPassTunnel::Quic(quic) => {
                        if quic.quic.is_none() {
                            quic.quic = Some(config.quic[0].quic_name.clone())
                        }
                        let upstream_name = "quic_tunnel_".to_string()
                            + &quic.address.clone()
                            + "_"
                            + &quic.quic.clone().take().unwrap();
                        upstream_name
                    }
                },
                config_toml::ProxyPass::Tunnel2(tunnel2) => match tunnel2 {
                    config_toml::ProxyPassTunnel2::Tcp(tcp) => {
                        if tcp.tcp.is_none() {
                            tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                        }
                        let upstream_name = "tcp_tunnel2_".to_string()
                            + &tcp.address.clone()
                            + "_"
                            + &tcp.tcp.clone().take().unwrap();
                        upstream_name
                    }
                    config_toml::ProxyPassTunnel2::Quic(quic) => {
                        if quic.quic.is_none() {
                            quic.quic = Some(config.quic[0].quic_name.clone())
                        }
                        let upstream_name = "quic_tunnel2_".to_string()
                            + &quic.address.clone()
                            + "_"
                            + &quic.quic.clone().take().unwrap();
                        upstream_name
                    }
                },
                config_toml::ProxyPass::Upstream(upstream_name) => {
                    v.is_upstream = true;
                    upstream_name.ups_name.clone()
                }
            };
            v.proxy_pass_upstream = upstream_name;
        }
        Ok(())
    };
    tcp_server_func().map_err(|e| anyhow!("err:tcp_server_func => e:{}", e))?;

    let mut domain_server_func = || -> Result<()> {
        if config._domain.is_none() {
            return Ok(());
        }

        let _domain = config._domain.as_mut().unwrap();
        if _domain.tcp.is_none() {
            _domain.tcp = Some(config.tcp[0].tcp_name.clone());
        }

        if _domain.quic.is_none() {
            _domain.quic = Some(config.quic[0].quic_name.clone());
        }

        if _domain.tunnel2.is_none() {
            _domain.tunnel2 = Some(config.tunnel2.clone());
        }

        if _domain.stream.is_none() {
            _domain.stream = Some(config.stream.clone());
        }

        if _domain.rate.is_none() {
            _domain.rate = Some(config.rate.clone());
        }

        if _domain.tmp_file.is_none() {
            _domain.tmp_file = Some(config.tmp_file.clone());
        }

        if _domain.fast_conf.is_none() {
            _domain.fast_conf = Some(config.fast_conf.clone());
        }

        for v in _domain._server.iter_mut() {
            v.common = Some(config.common.clone());

            if v.tcp.is_none() {
                v.tcp = Some(_domain.tcp.clone().unwrap());
            }

            if v.quic.is_none() {
                v.quic = Some(_domain.quic.clone().unwrap());
            }

            if v.tunnel2.is_none() {
                v.tunnel2 = Some(_domain.tunnel2.clone().unwrap());
            }

            if v.stream.is_none() {
                v.stream = Some(_domain.stream.clone().unwrap());
            }

            if v.rate.is_none() {
                v.rate = Some(_domain.rate.clone().unwrap());
            }

            if v.tmp_file.is_none() {
                v.tmp_file = Some(_domain.tmp_file.clone().unwrap());
            }

            if v.fast_conf.is_none() {
                v.fast_conf = Some(_domain.fast_conf.clone().unwrap());
            }

            if v.access.is_none() {
                v.access = Some(_domain.access.clone());
            }

            if v.listen.is_none() {
                v.listen = _domain.listen.clone();

                if v.listen.is_none() {
                    let err = anyhow!("err:domain:{} listen is null", v.domain);
                    log::error!("{}", err);
                    return Err(err);
                }
            }

            let upstream_name = match &mut v.proxy_pass {
                config_toml::ProxyPass::Tcp(tcp) => {
                    if tcp.tcp.is_none() {
                        tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                    }
                    let upstream_name = "tcp_".to_string()
                        + &tcp.address.clone()
                        + "_"
                        + &tcp.tcp.clone().take().unwrap();
                    upstream_name
                }
                config_toml::ProxyPass::Quic(quic) => {
                    if quic.quic.is_none() {
                        quic.quic = Some(config.quic[0].quic_name.clone())
                    }
                    let upstream_name = "quic_".to_string()
                        + &quic.address.clone()
                        + "_"
                        + &quic.quic.clone().take().unwrap();
                    upstream_name
                }
                config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                    config_toml::ProxyPassTunnel::Tcp(tcp) => {
                        if tcp.tcp.is_none() {
                            tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                        }
                        let upstream_name = "tcp_tunnel_".to_string()
                            + &tcp.address.clone()
                            + "_"
                            + &tcp.tcp.clone().take().unwrap();
                        upstream_name
                    }
                    config_toml::ProxyPassTunnel::Quic(quic) => {
                        if quic.quic.is_none() {
                            quic.quic = Some(config.quic[0].quic_name.clone())
                        }
                        let upstream_name = "quic_tunnel_".to_string()
                            + &quic.address.clone()
                            + "_"
                            + &quic.quic.clone().take().unwrap();
                        upstream_name
                    }
                },
                config_toml::ProxyPass::Tunnel2(tunnel2) => match tunnel2 {
                    config_toml::ProxyPassTunnel2::Tcp(tcp) => {
                        if tcp.tcp.is_none() {
                            tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                        }
                        let upstream_name = "tcp_tunnel2_".to_string()
                            + &tcp.address.clone()
                            + "_"
                            + &tcp.tcp.clone().take().unwrap();
                        upstream_name
                    }
                    config_toml::ProxyPassTunnel2::Quic(quic) => {
                        if quic.quic.is_none() {
                            quic.quic = Some(config.quic[0].quic_name.clone())
                        }
                        let upstream_name = "quic_tunnel2_".to_string()
                            + &quic.address.clone()
                            + "_"
                            + &quic.quic.clone().take().unwrap();
                        upstream_name
                    }
                },
                config_toml::ProxyPass::Upstream(upstream_name) => {
                    v.is_upstream = true;
                    upstream_name.ups_name.clone()
                }
            };
            v.proxy_pass_upstream = upstream_name;
        }
        Ok(())
    };
    domain_server_func().map_err(|e| anyhow!("err:domain_server_func => e:{}", e))?;

    let mut upstream_server_func = || -> Result<()> {
        if config._upstream.is_none() {
            return Ok(());
        }

        for ups_config in config._upstream.as_mut().unwrap()._server.iter_mut() {
            for ups_proxy_pass in ups_config.proxy_pass.iter_mut() {
                match ups_proxy_pass {
                    config_toml::ProxyPass::Tcp(tcp) => {
                        if tcp.tcp.is_none() {
                            tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                        }
                    }
                    config_toml::ProxyPass::Quic(quic) => {
                        if quic.quic.is_none() {
                            quic.quic = Some(config.quic[0].quic_name.clone())
                        }
                    }
                    config_toml::ProxyPass::Tunnel(tunnel) => match tunnel {
                        config_toml::ProxyPassTunnel::Tcp(tcp) => {
                            if tcp.tcp.is_none() {
                                tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                            }
                        }
                        config_toml::ProxyPassTunnel::Quic(quic) => {
                            if quic.quic.is_none() {
                                quic.quic = Some(config.quic[0].quic_name.clone())
                            }
                        }
                    },
                    config_toml::ProxyPass::Tunnel2(tunnel2) => match tunnel2 {
                        config_toml::ProxyPassTunnel2::Tcp(tcp) => {
                            if tcp.tcp.is_none() {
                                tcp.tcp = Some(config.tcp[0].tcp_name.clone())
                            }
                        }
                        config_toml::ProxyPassTunnel2::Quic(quic) => {
                            if quic.quic.is_none() {
                                quic.quic = Some(config.quic[0].quic_name.clone())
                            }
                        }
                    },
                    config_toml::ProxyPass::Upstream(_) => continue,
                }
            }
        }

        Ok(())
    };
    upstream_server_func().map_err(|e| anyhow!("err:upstream_server_func => e:{}", e))?;

    Ok(config)
}
