use crate::config::config_toml::QuicConfig as Config;
use crate::quic::util as quic_util;
use crate::util::util::get_ports;
use anyhow::anyhow;
use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Endpoints {
    config: Config,
    reuseport: bool,
    endpoints: Vec<quinn::Endpoint>,
    endpoints_ipv6: Vec<quinn::Endpoint>,
    index: AtomicUsize,
}

impl Endpoints {
    pub fn new(config: &Config, reuseport: bool) -> Result<Endpoints> {
        let mut endpoints = Vec::with_capacity(50);
        let mut endpoints_ipv6 = Vec::with_capacity(50);
        let ports = get_ports(&config.quic_upstream_ports)
            .map_err(|e| anyhow!("err:get_ports => e:{}", e))?;
        for port in ports.iter() {
            let addr = format!("0.0.0.0:{}", port);
            let addr = addr
                .parse()
                .map_err(|e| anyhow!("err:bind {} => e:{}", addr, e))?;
            let endpoint = quic_util::endpoint(config, reuseport, Some(addr), true)
                .map_err(|e| anyhow!("err:quic_util::endpoint => e:{}", e))?;
            endpoints.push(endpoint);
        }

        for port in ports.iter() {
            let addr = format!("[::]:{}", port);
            let addr = addr
                .parse()
                .map_err(|e| anyhow!("err:bind {} => e:{}", addr, e))?;
            let endpoint = quic_util::endpoint(config, reuseport, Some(addr), false)
                .map_err(|e| anyhow!("err:quic_util::endpoint => e:{}", e))?;
            endpoints_ipv6.push(endpoint);
        }

        Ok(Endpoints {
            config: config.clone(),
            reuseport,
            endpoints,
            endpoints_ipv6,
            index: AtomicUsize::new(0),
        })
    }

    pub fn endpoint(&self, is_ipv4: bool) -> Result<quinn::Endpoint> {
        let endpoints = if is_ipv4 {
            &self.endpoints
        } else {
            &self.endpoints_ipv6
        };

        if endpoints.len() <= 0 {
            let endpoint = quic_util::endpoint(&self.config, self.reuseport, None, is_ipv4)
                .map_err(|e| anyhow!("err:quic_util::endpoint => e:{}", e))?;
            return Ok(endpoint);
        }

        let index = self.index.fetch_add(1, Ordering::Relaxed) % endpoints.len();
        Ok(endpoints[index].clone())
    }
}
