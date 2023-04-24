use super::upstream_config::UpstreamConfig;
use super::upstream_server::UpstreamServer;
use super::UpstreamData;
use crate::config::config_toml;
use crate::TunnelClients;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::executor_local_spawn::ExecutorsLocal;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub struct Upstream {
    executor_local_spawn: ExecutorLocalSpawn,
    tunnel_clients: TunnelClients,
    tcp_config_map: Rc<HashMap<String, config_toml::TcpConfig>>,
    quic_config_map: Rc<HashMap<String, config_toml::QuicConfig>>,
    upstream_data_map: HashMap<String, Arc<Mutex<UpstreamData>>>,
}

impl Upstream {
    pub fn new(executors: ExecutorsLocal, tunnel_clients: TunnelClients) -> Result<Upstream> {
        let executor_local_spawn = ExecutorLocalSpawn::new(executors);
        Ok(Upstream {
            executor_local_spawn,
            tunnel_clients,
            tcp_config_map: Rc::new(HashMap::new()),
            quic_config_map: Rc::new(HashMap::new()),
            upstream_data_map: HashMap::new(),
        })
    }
    pub fn tcp_config(&self, name: &str) -> Option<config_toml::TcpConfig> {
        return self.tcp_config_map.get(name).cloned();
    }
    pub fn quic_config(&self, name: &str) -> Option<config_toml::QuicConfig> {
        return self.quic_config_map.get(name).cloned();
    }
    pub fn upstream_data(&self, name: &str) -> Option<Arc<Mutex<UpstreamData>>> {
        return self.upstream_data_map.get(name).cloned();
    }

    pub async fn start(
        &mut self,
        config: &config_toml::ConfigToml,
        ups_version: i32,
    ) -> Result<()> {
        log::info!("upstream start version:{}", ups_version);
        let ups_config =
            UpstreamConfig::new().map_err(|e| anyhow!("err:PortConfig::new => e:{}", e))?;
        let (tcp_config_map, quic_config_map, ups_data_map) = ups_config
            .parse_config(config, self.tunnel_clients.clone())
            .await
            .map_err(|e| anyhow!("err:port_config.parse_config => e:{}", e))?;

        self.tcp_config_map = tcp_config_map;
        self.quic_config_map = quic_config_map;
        self.upstream_data_map.clear();
        self.upstream_data_map = ups_data_map;

        for (_, ups_data) in self.upstream_data_map.iter() {
            let ups_data = ups_data.clone();
            self.executor_local_spawn._start(
                #[cfg(feature = "anyspawn-count")]
                format!("{}:{}", file!(), line!()),
                move |executors| async move {
                    let server = UpstreamServer::new(executors, ups_data)
                        .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                    Ok(())
                },
            );
        }
        Ok(())
    }
    pub async fn parse_config(&mut self, config: &config_toml::ConfigToml) -> Result<()> {
        if config._upstream.is_none() {
            return Ok(());
        }

        let ups_config =
            UpstreamConfig::new().map_err(|e| anyhow!("err:PortConfig::new => e:{}", e))?;

        let (tcp_config_map, quic_config_map, ups_data_map) = ups_config
            .parse_config(config, self.tunnel_clients.clone())
            .await
            .map_err(|e| anyhow!("err:port_config.parse_config => e:{}", e))?;

        self.tcp_config_map = tcp_config_map;
        self.quic_config_map = quic_config_map;
        self.upstream_data_map.clear();
        self.upstream_data_map = ups_data_map;
        Ok(())
    }

    pub async fn send(&self, flag: &str, is_fast_shutdown: bool) -> Result<()> {
        self.executor_local_spawn.send(flag, is_fast_shutdown);
        Ok(())
    }

    pub async fn stop(
        &self,
        flag: &str,
        is_fast_shutdown: bool,
        shutdown_timeout: u64,
        ups_version: i32,
    ) -> Result<()> {
        scopeguard::defer! {
            log::info!("end upstream stop version:{}", ups_version);
        }
        log::info!("start upstream stop version:{}", ups_version);
        self.executor_local_spawn
            .stop(flag, is_fast_shutdown, shutdown_timeout)
            .await;
        Ok(())
    }
}
