use crate::config::config_toml;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::stream_flow;
use crate::upstream::upstream;
use crate::util::var;
use crate::TunnelClients;
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;
use std::rc::Rc;

#[async_trait(?Send)]
pub trait Proxy {
    async fn start(
        &mut self,
        config: &config_toml::ConfigToml,
        ups: Rc<upstream::Upstream>,
    ) -> Result<()>;
    async fn stop(&self, flag: &str, is_fast_shutdown: bool, shutdown_timeout: u64) -> Result<()>;
    async fn send(&self, flag: &str, is_fast_shutdown: bool) -> Result<()>;
    async fn wait(&self, flag: &str) -> Result<()>;
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

#[async_trait(?Send)]
pub trait Stream {
    async fn do_start(
        &mut self,
        stream_info: Rc<RefCell<StreamInfo>>,
        stream: stream_flow::StreamFlow,
    ) -> Result<()>;
}

pub struct AccessContext {
    pub access_format_vars: var::Var,
    pub access_log_file: std::sync::Arc<std::fs::File>,
}
