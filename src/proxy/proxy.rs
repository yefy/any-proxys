use crate::config::config_toml;
use crate::util::var;
use any_tunnel::client as tunnel_client;
use any_tunnel2::client as tunnel2_client;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait Proxy {
    async fn start(&mut self, config: &config_toml::ConfigToml) -> anyhow::Result<()>;
    async fn stop(&self, _: bool) -> anyhow::Result<()>;
    async fn wait(&self) -> anyhow::Result<()>;
}

#[async_trait(?Send)]
pub trait Config {
    async fn parse(
        &self,
        config: &config_toml::ConfigToml,
        tunnel_client: tunnel_client::Client,
        tunnel2_client: tunnel2_client::Client,
    ) -> anyhow::Result<()>;
}

pub struct AccessContext {
    pub access_format_vars: var::Var,
    pub access_log_file: std::sync::Arc<std::fs::File>,
}
