use crate::config::config_toml;
use crate::upstream::upstream;
use crate::util::var;
use crate::TunnelClients;
use anyhow::Result;
use async_trait::async_trait;
use std::rc::Rc;

#[async_trait(?Send)]
pub trait Proxy {
    async fn start(
        &mut self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
    ) -> Result<()>;
    async fn stop(&self, _: bool) -> Result<()>;
    async fn wait(&self) -> Result<()>;
}

#[async_trait(?Send)]
pub trait Config {
    async fn parse(
        &self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
        tunnel_clients: TunnelClients,
    ) -> Result<()>;
}

pub struct AccessContext {
    pub access_format_vars: var::Var,
    pub access_log_file: std::sync::Arc<std::fs::File>,
}
