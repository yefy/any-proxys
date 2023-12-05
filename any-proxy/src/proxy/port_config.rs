use super::StreamConfigContext;
use crate::stream::server;
use any_base::typ::ShareRw;
use anyhow::anyhow;
use anyhow::Result;
use std::sync::Arc;

pub struct PortConfigContext {
    pub scc: ShareRw<StreamConfigContext>,
}

#[derive(Clone)]
pub struct PortConfigListen {
    pub listen_server: Arc<Box<dyn server::Server>>,
    pub port_config_context: Arc<PortConfigContext>,
}

pub struct PortConfig {}

impl PortConfig {
    pub fn new() -> Result<PortConfig> {
        Ok(PortConfig {})
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
