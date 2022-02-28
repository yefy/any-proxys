use super::config_toml;
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
    ];
}

pub fn check(mut config: config_toml::ConfigToml) -> anyhow::Result<config_toml::ConfigToml> {
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

    if config.common.config_log_stdout {
        log::info!("config = {:#?}", config);
    }

    Ok(config)
}

pub fn merger(mut config: config_toml::ConfigToml) -> anyhow::Result<config_toml::ConfigToml> {
    let mut tcp_server_func = || -> anyhow::Result<()> {
        if config._port.is_none() {
            return Ok(())
        }

        let _port = config._port.as_mut().unwrap();
        if _port.tcp.is_none() {
            _port.tcp = Some(config.tcp.clone());
        }

        if _port.quic.is_none() {
            _port.quic = Some(config.quic.clone());
        }

        if _port.tunnel2.is_none() {
            _port.tunnel2 = Some(config.tunnel2.clone());
        }

        if _port.stream.is_none() {
            _port.stream = Some(config.stream.clone());
        }

        for v in _port._server.iter_mut() {
            v.common = Some(config.common.clone());
            if v.tcp.is_none() {
                v.tcp = Some(_port.tcp.clone().unwrap())
            }

            // if v.quic.is_none() {
            //     v.quic = Some(config._port.quic.clone().unwrap())
            // }

            if v.tunnel2.is_none() {
                v.tunnel2 = Some(_port.tunnel2.clone().unwrap())
            }

            if v.stream.is_none() {
                v.stream = Some(_port.stream.clone().unwrap())
            }

            if v.access.is_none() {
                v.access = Some(_port.access.clone())
            }
        }
        Ok(())
    };
    tcp_server_func()?;

    let mut domain_server_func = || -> anyhow::Result<()> {
        if config._domain.is_none() {
            return Ok(())
        }

        let _domain = config._domain.as_mut().unwrap();
        if _domain.tcp.is_none() {
            _domain.tcp = Some(config.tcp.clone());
        }

        if _domain.quic.is_none() {
            _domain.quic = Some(config.quic.clone());
        }

        if _domain.tunnel2.is_none() {
            _domain.tunnel2 = Some(config.tunnel2.clone());
        }

        if _domain.stream.is_none() {
            _domain.stream = Some(config.stream.clone());
        }

        for v in _domain._server.iter_mut() {
            v.common = Some(config.common.clone());

            if v.tcp.is_none() {
                v.tcp = Some(_domain.tcp.clone().unwrap());
            }

            // if v.quic.is_none() {
            //     v.quic = Some(config._domain.quic.clone().unwrap());
            // }

            if v.tunnel2.is_none() {
                v.tunnel2 = Some(_domain.tunnel2.clone().unwrap());
            }

            if v.stream.is_none() {
                v.stream = Some(_domain.stream.clone().unwrap());
            }

            if v.access.is_none() {
                v.access = Some(_domain.access.clone());
            }

            if v.listen.is_none() {
                v.listen = _domain.listen.clone();

                if v.listen.is_none() {
                    let err = anyhow::anyhow!("err:domain:{} listen is null", v.domain);
                    log::error!("{}", err);
                    return Err(err);
                }
            }
        }
        Ok(())
    };
    domain_server_func()?;

    Ok(config)
}
