use super::upstream_config::UpstreamConfig;
use super::upstream_server::UpstreamServer;
use super::UpstreamData;
use crate::config::config_toml;
use crate::Executors;
use crate::TunnelClients;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WaitGroup;
use futures_util::task::LocalSpawnExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::broadcast;

pub struct Upstream {
    executors: Executors,
    tunnel_clients: TunnelClients,
    tcp_config_map: Rc<HashMap<String, config_toml::TcpConfig>>,
    quic_config_map: Rc<HashMap<String, config_toml::QuicConfig>>,
    upstream_data_map: HashMap<String, Rc<RefCell<Option<UpstreamData>>>>,
    ups_server_wait_group: WaitGroup,
}

impl Upstream {
    pub fn new(executors: Executors, tunnel_clients: TunnelClients) -> Result<Upstream> {
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let executors = Executors {
            executor: executors.executor.clone(),
            shutdown_thread_tx,
            group_version: executors.group_version,
            thread_id: executors.thread_id,
        };

        Ok(Upstream {
            executors,
            tunnel_clients,
            tcp_config_map: Rc::new(HashMap::new()),
            quic_config_map: Rc::new(HashMap::new()),
            upstream_data_map: HashMap::new(),
            ups_server_wait_group: WaitGroup::new(),
        })
    }
    pub fn tcp_config(&self, name: &str) -> Option<config_toml::TcpConfig> {
        return self.tcp_config_map.get(name).cloned();
    }
    pub fn quic_config(&self, name: &str) -> Option<config_toml::QuicConfig> {
        return self.quic_config_map.get(name).cloned();
    }
    pub fn upstream_data(&self, name: &str) -> Option<Rc<RefCell<Option<UpstreamData>>>> {
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
            let worker_group = self.ups_server_wait_group.worker();
            let worker = self.ups_server_wait_group.worker().add();
            let executors = self.executors.clone();
            let ups_data = ups_data.clone();
            executors
                .executor
                .clone()
                .spawn_local(async move {
                    scopeguard::defer! {
                        worker.done();
                    }

                    let ret: Result<()> = async {
                        let server = UpstreamServer::new(executors, worker_group, ups_data)
                            .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                        server
                            .start()
                            .await
                            .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                        Ok(())
                    }
                    .await;
                    ret.unwrap_or_else(|e| log::error!("err:Port.start => e:{}", e));
                })
                .unwrap_or_else(|e| log::error!("{}", e));
        }
        Ok(())
    }
    pub async fn parse_config(&mut self, config: &config_toml::ConfigToml) -> Result<()> {
        if config.upstream.is_none() {
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

    pub async fn stop(&self, is_fast_shutdown: bool) -> Result<()> {
        let _ = self.executors.shutdown_thread_tx.send(is_fast_shutdown)?;
        Ok(())
    }
    pub async fn wait(&self, ups_version: i32) -> Result<()> {
        self.ups_server_wait_group.wait().await;
        log::info!("upstream stop version:{}", ups_version);
        Ok(())
    }
}
