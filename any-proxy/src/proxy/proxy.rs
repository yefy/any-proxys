use crate::config::config_toml;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::stream_flow;
use crate::upstream::upstream;
use crate::util::var;
use crate::{Protocol7, TunnelClients};
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
    async fn stop(&self, _: bool) -> Result<()>;
    async fn send(&self, _: bool) -> Result<()>;
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
        protocol7: Protocol7,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_stream: stream_flow::StreamFlow,
    ) -> Result<()>;
}

pub struct AccessContext {
    pub access_format_vars: var::Var,
    pub access_log_file: std::sync::Arc<std::fs::File>,
}
