use super::StreamConfigContext;
use crate::config::config_toml::Listen;
use crate::stream::server::Server;
use crate::util;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct DomainConfigContext {
    pub scc: ShareRw<StreamConfigContext>,
}

#[derive(Clone)]
pub struct DomainConfigListen {
    pub listen_server: Arc<Box<dyn Server>>,
    pub domain_config_context_map: HashMap<i32, Arc<DomainConfigContext>>,
    pub domain_index: Arc<util::domain_index::DomainIndex>,
    pub sni: Option<util::Sni>,
    pub plugin_handle_protocol: ArcRwLockTokio<PluginHandleProtocol>,
}

use std::future::Future;
use std::pin::Pin;
type DomainConfigFunc =
    fn(ArcMutex<DomainConfigListenMerge>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
use crate::config::net_server_core_plugin::PluginHandleProtocol;
use any_base::module::module;
use any_base::typ::{ArcMutex, ArcRwLockTokio, ShareRw};

#[derive(Clone)]
pub struct DomainConfigListenMerge {
    pub ms: module::Modules,
    pub key: String,
    pub listen_addr: Option<SocketAddr>,
    pub listens: Vec<Listen>,
    pub domain_config_contexts: Vec<Arc<DomainConfigContext>>,
    pub func: DomainConfigFunc,
}

pub struct DomainConfig {}

impl DomainConfig {
    pub fn new() -> Result<DomainConfig> {
        Ok(DomainConfig {})
    }

    pub fn merger(
        old_domain_config_listen: &DomainConfigListen,
        mut new_domain_config_listen: DomainConfigListen,
    ) -> Result<DomainConfigListen> {
        if old_domain_config_listen.sni.is_some() && new_domain_config_listen.sni.is_some() {
            let old_sni = old_domain_config_listen.sni.as_ref().unwrap();
            old_sni.take_from(new_domain_config_listen.sni.as_ref().unwrap());
            new_domain_config_listen.sni = Some(old_sni.clone());
        }
        Ok(new_domain_config_listen)
    }
}
