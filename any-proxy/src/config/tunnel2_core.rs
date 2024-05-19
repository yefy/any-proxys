use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use any_tunnel2::client;
use any_tunnel2::server;
use any_tunnel2::tunnel;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn default_tunnel2_worker_thread() -> usize {
    0
}
fn default_tunnel2_max_connect() -> usize {
    100
}
fn default_is_open_tunnel2() -> bool {
    false
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tunnel2Config {
    #[serde(default = "default_tunnel2_worker_thread")]
    pub tunnel2_worker_thread: usize,
    #[serde(default = "default_tunnel2_max_connect")]
    pub tunnel2_max_connect: usize,
    #[serde(default = "default_is_open_tunnel2")]
    pub is_open_tunnel2: bool,
}

impl Tunnel2Config {
    pub fn new() -> Self {
        Tunnel2Config {
            tunnel2_worker_thread: default_tunnel2_worker_thread(),
            tunnel2_max_connect: default_tunnel2_max_connect(),
            is_open_tunnel2: default_is_open_tunnel2(),
        }
    }
}

pub struct Conf {
    tunnel2_conf: Tunnel2Config,
    client: Option<client::Client>,
    server: Option<server::Server>,
    peer_stream_max_len: Arc<AtomicUsize>,
}

impl Drop for Conf {
    fn drop(&mut self) {
        log::debug!(target: "ms", "drop tunnel2_core");
    }
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            tunnel2_conf: Tunnel2Config::new(),
            client: None,
            server: None,
            peer_stream_max_len: Arc::new(AtomicUsize::new(1)),
        }
    }
    pub fn client(&self) -> Option<client::Client> {
        if self.client.is_none() {
            None
        } else {
            self.client.clone()
        }
    }
    pub fn server(&self) -> Option<server::Server> {
        if self.server.is_none() {
            None
        } else {
            self.server.clone()
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "data".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(data(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_DATA,
        conf_typ: conf::CMD_CONF_TYPE_MAIN,
    },]);
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
        name: "tunnel_core2".to_string(),
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
        typ: conf::MODULE_TYPE_TUNNEL2,
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
    old_conf: Option<typ::ArcUnsafeAny>,
    _ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    if conf.tunnel2_conf.is_open_tunnel2 {
        if old_conf.is_some()
            && old_conf
                .as_ref()
                .unwrap()
                .get_mut::<Conf>()
                .tunnel2_conf
                .is_open_tunnel2
        {
            let old_conf = old_conf.as_ref().unwrap().get_mut::<Conf>();
            conf.client = old_conf.client.clone();
            conf.server = old_conf.server.clone();
            conf.peer_stream_max_len = old_conf.peer_stream_max_len.clone();
            conf.peer_stream_max_len
                .store(conf.tunnel2_conf.tunnel2_max_connect, Ordering::SeqCst);
        } else {
            let peer_stream_max_len =
                Arc::new(AtomicUsize::new(conf.tunnel2_conf.tunnel2_max_connect));
            let client = tunnel::Tunnel::start_thread(
                "client".to_string(),
                conf.tunnel2_conf.tunnel2_worker_thread,
            )
            .await;
            let client = client::Client::new(client, peer_stream_max_len.clone());

            let server = tunnel::Tunnel::start_thread(
                "server".to_string(),
                conf.tunnel2_conf.tunnel2_worker_thread,
            )
            .await;
            let server = server::Server::new(server);
            conf.client = Some(client);
            conf.server = Some(server);
            conf.peer_stream_max_len = peer_stream_max_len;
        }
    }

    return Ok(());
}

async fn init_conf(
    _ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    if conf.tunnel2_conf.tunnel2_worker_thread == 0 {
        let core_ids = core_affinity::get_core_ids().unwrap();
        conf.tunnel2_conf.tunnel2_worker_thread = core_ids.len();
    }
    if conf.tunnel2_conf.tunnel2_worker_thread == 0 {
        conf.tunnel2_conf.tunnel2_worker_thread = 1;
    }
    return Ok(());
}

async fn data(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let tunnel2_conf: Tunnel2Config =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    conf.tunnel2_conf = tunnel2_conf;
    return Ok(());
}
