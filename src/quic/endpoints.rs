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
    index: AtomicUsize,
}

impl Endpoints {
    pub fn new(config: &Config, reuseport: bool) -> Result<Endpoints> {
        let mut endpoints = Vec::with_capacity(50);
        let ports = get_ports(&config.quic_upstream_ports)
            .map_err(|e| anyhow!("err:get_ports => e:{}", e))?;
        for port in ports.iter() {
            let addr = format!("0.0.0.0:{}", port);
            let addr = addr
                .parse()
                .map_err(|e| anyhow!("err:bind {} => e:{}", addr, e))?;
            let endpoint = quic_util::endpoint(config, reuseport, Some(addr))
                .map_err(|e| anyhow!("err:quic_util::endpoint => e:{}", e))?;
            endpoints.push(endpoint);
        }

        Ok(Endpoints {
            config: config.clone(),
            reuseport,
            endpoints,
            index: AtomicUsize::new(0),
        })
    }

    pub fn endpoint(&self) -> Result<quinn::Endpoint> {
        if self.endpoints.len() <= 0 {
            let endpoint = quic_util::endpoint(&self.config, self.reuseport, None)
                .map_err(|e| anyhow!("err:quic_util::endpoint => e:{}", e))?;
            return Ok(endpoint);
        }

        let index = self.index.fetch_add(1, Ordering::Relaxed) % self.endpoints.len();
        Ok(self.endpoints[index].clone())
    }
}
