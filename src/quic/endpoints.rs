use crate::config::config_toml::QuicConfig as Config;
use crate::quic::util as quic_util;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Endpoints {
    endpoints: Vec<quinn::Endpoint>,
    index: AtomicUsize,
}

impl Endpoints {
    pub fn new(config: &Config, reuseport: bool) -> anyhow::Result<Endpoints> {
        let mut endpoints = Vec::with_capacity(50);
        for _ in 0..config.quic_upstream_keepalive {
            let endpoint = quic_util::endpoint(config, reuseport)?;
            endpoints.push(endpoint);
        }

        Ok(Endpoints {
            endpoints,
            index: AtomicUsize::new(0),
        })
    }

    pub fn endpoint(&self) -> quinn::Endpoint {
        let index = self.index.fetch_add(1, Ordering::Relaxed) % self.endpoints.len();
        self.endpoints[index].clone()
    }
}
