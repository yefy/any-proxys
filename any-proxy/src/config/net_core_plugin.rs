use crate::config as conf;
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::{ServerArg, StreamStatus, StreamStreamShare};
use any_base::module::module;
use any_base::stream_flow::StreamFlow;
use any_base::typ;
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny, Share};
use anyhow::Result;
use lazy_static::lazy_static;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_tungstenite::WebSocketStream;

pub type IsPluginHandleAccess =
    fn(Share<StreamInfo>) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>>;

pub type PluginHandleAccess =
    fn(Share<StreamInfo>) -> Pin<Box<dyn Future<Output = Result<crate::Error>> + Send>>;

pub type PluginHandleLog =
    fn(Share<StreamInfo>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHandleStream =
    fn(Arc<StreamStreamShare>) -> Pin<Box<dyn Future<Output = Result<StreamStatus>> + Send>>;

pub type PluginHandleSetStream =
    fn(plugin: PluginHandleStream) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHttpFilter =
    fn(Arc<HttpStreamRequest>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHttpFilterSet =
    fn(plugin: PluginHttpFilter) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHttpIn =
    fn(Arc<HttpStreamRequest>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHttpInSet =
    fn(plugin: PluginHttpIn) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub type PluginHandleHttp =
    fn(Arc<HttpStreamRequest>) -> Pin<Box<dyn Future<Output = Result<crate::Error>> + Send>>;

use any_base::io::buf_stream::BufStream;
pub type PluginHandleWebsocket = fn(
    arg: ServerArg,
    client_stream: ArcMutex<WebSocketStream<BufStream<StreamFlow>>>,
) -> Pin<Box<dyn Future<Output = Result<crate::Error>> + Send>>;

pub struct Conf {
    pub plugin_handle_access: ArcRwLockTokio<Vec<PluginHandleAccess>>,
    pub plugin_handle_http_access: ArcRwLockTokio<Vec<PluginHandleAccess>>,
    pub is_plugin_handle_serverless: ArcRwLockTokio<Vec<IsPluginHandleAccess>>,
    pub plugin_handle_serverless: ArcRwLockTokio<Vec<PluginHandleAccess>>,
    pub plugin_handle_logs: ArcRwLockTokio<Vec<PluginHandleLog>>,
    pub plugin_handle_stream_cache: ArcRwLockTokio<PluginHandleStream>,
    pub plugin_handle_set_stream_cache: ArcRwLockTokio<PluginHandleSetStream>,
    pub plugin_handle_stream_memory: ArcRwLockTokio<PluginHandleStream>,
    pub plugin_handle_set_stream_memory: ArcRwLockTokio<PluginHandleSetStream>,

    pub plugin_http_header_in: ArcRwLockTokio<PluginHttpIn>,
    pub plugin_http_header_in_set: ArcRwLockTokio<PluginHttpInSet>,
    pub plugin_http_header_filter: ArcRwLockTokio<PluginHttpFilter>,
    pub plugin_http_header_filter_set: ArcRwLockTokio<PluginHttpFilterSet>,
    pub plugin_http_body_filter: ArcRwLockTokio<PluginHttpFilter>,
    pub plugin_http_body_filter_set: ArcRwLockTokio<PluginHttpFilterSet>,

    pub plugin_handle_http: ArcRwLockTokio<Vec<PluginHandleHttp>>,
    pub plugin_handle_websocket: ArcRwLockTokio<Vec<PluginHandleWebsocket>>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            plugin_handle_access: ArcRwLockTokio::new(Vec::new()),
            plugin_handle_http_access: ArcRwLockTokio::new(Vec::new()),
            is_plugin_handle_serverless: ArcRwLockTokio::new(Vec::new()),
            plugin_handle_serverless: ArcRwLockTokio::new(Vec::new()),
            plugin_handle_logs: ArcRwLockTokio::new(Vec::new()),
            plugin_handle_stream_cache: ArcRwLockTokio::default(),
            plugin_handle_set_stream_cache: ArcRwLockTokio::default(),
            plugin_handle_stream_memory: ArcRwLockTokio::default(),
            plugin_handle_set_stream_memory: ArcRwLockTokio::default(),

            plugin_http_header_in: ArcRwLockTokio::default(),
            plugin_http_header_in_set: ArcRwLockTokio::default(),
            plugin_http_header_filter: ArcRwLockTokio::default(),
            plugin_http_header_filter_set: ArcRwLockTokio::default(),
            plugin_http_body_filter: ArcRwLockTokio::default(),
            plugin_http_body_filter_set: ArcRwLockTokio::default(),
            plugin_handle_http: ArcRwLockTokio::new(Vec::new()),
            plugin_handle_websocket: ArcRwLockTokio::new(Vec::new()),
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![]);
}

lazy_static! {
    pub static ref MODULE_FUNC: Arc<module::Func> = Arc::new(module::Func {
        create_conf: |ms| Box::pin(create_conf(ms)),
        merge_conf: |ms, parent_conf, child_conf| Box::pin(merge_conf(ms, parent_conf, child_conf)),

        init_conf: |ms, parent_conf, child_conf| Box::pin(init_conf(ms, parent_conf, child_conf)),
        merge_old_conf: |old_ms, old_main_conf, old_conf, ms, main_conf, conf| Box::pin(
            merge_old_conf(old_ms, old_main_conf, old_conf, ms, main_conf, conf)
        ),
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_core_plugin".to_string(),
        main_index: -1,
        ctx_index: -1,
        index: -1,
        ctx_index_len: -1,
        func: MODULE_FUNC.clone(),
        cmds: MODULE_CMDS.clone(),
        create_main_confs: None,
        init_main_confs: None,
        merge_old_main_confs: None,
        merge_confs: None,
        init_master_thread_confs: None,
        init_work_thread_confs: None,
        drop_confs: None,
        typ: conf::MODULE_TYPE_NET,
        create_server: None,
    });
}

pub fn module() -> typ::ArcRwLock<module::Module> {
    return M.clone();
}

pub async fn main_conf(ms: &module::Modules) -> &Conf {
    ms.get_main_conf::<Conf>(module()).await
}

pub async fn main_conf_mut(ms: &module::Modules) -> &mut Conf {
    ms.get_main_conf_mut::<Conf>(module()).await
}

pub async fn main_any_conf(ms: &module::Modules) -> ArcUnsafeAny {
    ms.get_main_any_conf(module()).await
}

pub fn curr_conf(curr: &ArcUnsafeAny) -> &Conf {
    module::Modules::get_curr_conf(curr, module())
}

pub fn curr_conf_mut(curr: &ArcUnsafeAny) -> &mut Conf {
    module::Modules::get_curr_conf_mut(curr, module())
}

pub fn currs_conf(curr: &Vec<ArcUnsafeAny>) -> &Conf {
    module::Modules::get_currs_conf(curr, module())
}

pub fn currs_conf_mut(curr: &Vec<ArcUnsafeAny>) -> &mut Conf {
    module::Modules::get_currs_conf_mut(curr, module())
}

pub fn curr_any_conf(curr: &ArcUnsafeAny) -> ArcUnsafeAny {
    module::Modules::get_curr_any_conf(curr, module())
}

pub fn currs_any_conf(curr: &Vec<ArcUnsafeAny>) -> ArcUnsafeAny {
    module::Modules::get_currs_any_conf(curr, module())
}

async fn create_conf(_ms: module::Modules) -> Result<typ::ArcUnsafeAny> {
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new())));
}

async fn merge_conf(
    _ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    if parent_conf.is_some() {
        let mut _parent_conf = parent_conf.unwrap().get_mut::<Conf>();
    }
    let mut _child_conf = child_conf.get_mut::<Conf>();
    return Ok(());
}

async fn merge_old_conf(
    _old_ms: Option<module::Modules>,
    _old_main_conf: Option<typ::ArcUnsafeAny>,
    _old_conf: Option<typ::ArcUnsafeAny>,
    _ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}

async fn init_conf(
    _ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _conf = conf.get_mut::<Conf>();
    return Ok(());
}
