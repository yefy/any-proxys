use crate::config as conf;
use crate::config::common_core::{get_session, get_tmp_file};
use crate::proxy::http_proxy::http_cache_file::{ProxyCache, ProxyCacheFileNodeData};
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::http_proxy::util::timer_check_proxy_cache;
use crate::proxy::stream_info::StreamInfo;
use crate::util::var::Var;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::module::module;
use any_base::parking_lot::typ::ArcMutex;
use any_base::typ;
use any_base::typ::{ArcRwLock, ArcUnsafeAny, Share};
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use lazy_static::lazy_static;
use radix_trie::Trie;
use serde::Deserialize;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub struct ProxyRewrite {
    pub regex: regex::Regex,
    pub vars: Var,
    pub status: http::StatusCode,
}

impl ProxyRewrite {
    pub fn start(
        &self,
        r: &Arc<HttpStreamRequest>,
        stream_info: &Share<StreamInfo>,
    ) -> Result<Option<(String, http::StatusCode)>> {
        let url = r.ctx.get().r_in.uri.to_string();
        let caps = self.regex.captures(&url);
        if caps.is_none() {
            return Ok(None);
        }
        let caps = caps.map(|cap| {
            cap.iter()
                .filter_map(|m| m.map(|mat| mat.as_str().to_string()))
                .collect::<Vec<String>>()
        });
        if caps.is_none() {
            return Ok(None);
        }

        let old_caps = unsafe { stream_info.get_mut().caps.take_op() };
        stream_info.get_mut().caps = Some(caps.unwrap()).into();

        let str = {
            let stream_info = stream_info.get();
            let mut vars =
                Var::copy(&self.vars).map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            use crate::proxy::stream_var;
            vars.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &stream_info)
                    .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                Ok(value)
            })?;
            let str = vars
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            str
        };

        stream_info.get_mut().caps = old_caps.into();

        Ok(Some((str, self.status)))
    }
}

fn default_proxy_cache_conf_is_open() -> bool {
    true
}
fn default_proxy_cache_conf_levels() -> String {
    "2:2".to_string()
}
fn default_proxy_cache_conf_max_size() -> i64 {
    0
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyCacheConf {
    #[serde(default = "default_proxy_cache_conf_is_open")]
    pub is_open: bool,
    pub name: String,
    pub path: String,
    #[serde(default = "default_proxy_cache_conf_levels")]
    pub levels: String,
    #[serde(default = "default_proxy_cache_conf_max_size")]
    pub max_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyCacheValidConf {
    pub proxy_cache_valid: Vec<ProxyCacheValid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyCacheValid {
    pub status: Vec<String>,
    pub time: u64,
}

fn proxy_hot_file_is_open() -> bool {
    false
}
fn proxy_hot_file_hot_interval_time() -> u64 {
    60 * 5
}

fn proxy_hot_file_hot_top_count() -> u64 {
    50
}

fn proxy_hot_file_hot_read_min_count() -> u64 {
    100
}

fn proxy_hot_file_hot_io_percent() -> u64 {
    80
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyHotFile {
    #[serde(default = "proxy_hot_file_is_open")]
    pub is_open: bool,
    #[serde(default = "proxy_hot_file_hot_interval_time")]
    pub hot_interval_time: u64,
    #[serde(default = "proxy_hot_file_hot_top_count")]
    pub hot_top_count: u64,
    #[serde(default = "proxy_hot_file_hot_read_min_count")]
    pub hot_read_min_count: u64,
    #[serde(default = "proxy_hot_file_hot_io_percent")]
    pub hot_io_percent: u64,
}

impl ProxyHotFile {
    pub fn new() -> Self {
        ProxyHotFile {
            is_open: proxy_hot_file_is_open(),
            hot_interval_time: proxy_hot_file_hot_interval_time(),
            hot_top_count: proxy_hot_file_hot_top_count(),
            hot_read_min_count: proxy_hot_file_hot_read_min_count(),
            hot_io_percent: proxy_hot_file_hot_io_percent(),
        }
    }
}

pub const DEFAULT_PROXY_EXPIRES_FILE_TIMER: u64 = 10;
pub const CACHE_FILE_SLISE: u64 = 1024 * 1024;
const DEFAULT_PROXY_CACHE_KEY: &str =
    "${http_request_method}${http_request_domain}${http_request_uri}";
lazy_static! {
    pub static ref CHECK_METHODS_MAP: ArcMutex<HashMap<String, bool>> = {
        let mut data = HashMap::new();
        data.insert("get".to_string(), true);
        data.insert("post".to_string(), true);
        data.insert("put".to_string(), true);
        data.insert("delete".to_string(), true);
        data.insert("patch".to_string(), true);
        data.insert("head".to_string(), true);
        data.insert("options".to_string(), true);
        ArcMutex::new(data)
    };
}

pub struct ProxyCacheIndex {
    pub uri_trie: Trie<String, ArcRwLock<std::collections::HashSet<Bytes>>>,
    pub index_map: HashMap<Bytes, ArcRwLock<HashMap<String, u64>>>,
}

impl ProxyCacheIndex {
    pub fn new() -> Self {
        Self {
            uri_trie: Trie::new(),
            index_map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, url: String, md5: Bytes, proxy_cache_name: String, version: u64) {
        let value = self.uri_trie.get(&url).cloned();
        let value = if value.is_none() {
            let value = ArcRwLock::new(std::collections::HashSet::new());
            self.uri_trie.insert(url, value.clone());
            value
        } else {
            value.unwrap()
        };
        value.get_mut().insert(md5.clone());

        let value = self.index_map.get(&md5).cloned();
        let value = if value.is_none() {
            let value = ArcRwLock::new(HashMap::new());
            self.index_map.insert(md5, value.clone());
            value
        } else {
            value.unwrap()
        };
        value.get_mut().insert(proxy_cache_name, version);
    }

    pub fn del(&mut self, url: &String, md5: &Bytes, proxy_cache_name: &String, version: u64) {
        log::debug!(target: "purge", "del file url:{}, md5:{}, proxy_cache_name:{}, version:{}", url, String::from_utf8_lossy(md5), proxy_cache_name, version);
        let value = self.index_map.get(md5).cloned();
        if value.is_some() {
            let value = value.unwrap();
            let value = &mut *value.get_mut();
            let v = value.get(proxy_cache_name).cloned();
            if v.is_some() {
                let v = v.unwrap();
                log::debug!(target: "purge", "find version:{}", v);
                if v > version {
                    return;
                }
                value.remove(proxy_cache_name);
            }

            if !value.is_empty() {
                return;
            }
        }

        let value = self.uri_trie.get(url).cloned();
        if value.is_none() {
            return;
        }
        let value = value.unwrap();
        let value = &mut *value.get_mut();
        value.remove(md5);
        if value.is_empty() {
            self.uri_trie.remove(url);
        }
    }

    pub fn index_get_rand(&self, md5: &Bytes) -> Option<String> {
        let value = self.index_map.get(md5).cloned();
        if value.is_none() {
            return None;
        }
        let value = value.unwrap();
        let value = &*value.get();
        if value.is_empty() {
            return None;
        }
        use rand::Rng;
        let values = value.iter().map(|(data, _)| data).collect::<Vec<&String>>();
        let index: usize = rand::thread_rng().gen();
        let index = index % values.len();
        Some(values[index].to_string())
    }

    pub fn index_get(&self, md5: &Bytes) -> Option<Vec<String>> {
        let value = self.index_map.get(md5).cloned();
        if value.is_none() {
            return None;
        }
        let value = value.unwrap();
        let value = &*value.get();
        if value.is_empty() {
            return None;
        }
        let values = value
            .iter()
            .map(|(data, _)| data.clone())
            .collect::<Vec<String>>();
        Some(values)
    }

    pub fn index_contains(&self, md5: &Bytes, proxy_cache_name: &String) -> bool {
        let value = self.index_map.get(md5).cloned();
        if value.is_none() {
            return false;
        }
        let value = value.unwrap();
        let value = value.get();
        if value.is_none() {
            return false;
        }
        value.get(proxy_cache_name).is_some()
    }
}

#[derive(Clone)]
pub struct ConfMain {
    pub proxy_cache_confs: ArcRwLock<Vec<ProxyCacheConf>>,
    pub proxy_expires_file_timer: u64,
    pub proxy_max_open_file: usize,
    pub proxy_cache_map: ArcRwLock<HashMap<String, Arc<ProxyCache>>>,
    pub is_init_master_thread: Arc<AtomicBool>,
    pub cache_file_node_queue: ArcMutex<VecDeque<Arc<ProxyCacheFileNodeData>>>,
    pub hot_io_percent_map: ArcRwLock<HashMap<String, u64>>,
    pub proxy_cache_index: ArcRwLock<ProxyCacheIndex>,
    pub session_id: Arc<AtomicU64>,
    pub tmp_file_id: Arc<AtomicU64>,
}

pub struct Conf {
    pub proxy_cache_names: Option<Vec<String>>,
    pub proxy_cache_key: String,
    pub proxy_cache_key_vars: Var,
    pub proxy_cache_purge: bool,
    pub proxy_request_slice: u64,
    pub proxy_cache_methods: HashMap<String, bool>,
    pub proxy_cache_valids: HashMap<u16, u64>,
    pub proxy_caches: Vec<Arc<ProxyCache>>,
    pub proxy_hot_file: ProxyHotFile,
    pub proxy_get_to_get_range: bool,
    pub rewrite: Vec<ProxyRewrite>,
    pub main: ConfMain,
}

impl Drop for Conf {
    fn drop(&mut self) {
        log::debug!(target: "ms", "drop net_core_proxy");
    }
}

impl Conf {
    pub fn new() -> Self {
        let session_id = get_session();
        let tmp_file_id = get_tmp_file();
        Conf {
            proxy_caches: Vec::with_capacity(10),
            proxy_cache_names: None,
            proxy_request_slice: 1 * CACHE_FILE_SLISE,
            proxy_cache_key: DEFAULT_PROXY_CACHE_KEY.to_string(),
            proxy_cache_key_vars: Var::new(DEFAULT_PROXY_CACHE_KEY, "").unwrap(),
            proxy_cache_methods: HashMap::new(),
            proxy_cache_valids: HashMap::new(),
            proxy_cache_purge: false,
            proxy_hot_file: ProxyHotFile::new(),
            proxy_get_to_get_range: false,
            rewrite: Vec::new(),
            main: ConfMain {
                proxy_cache_map: ArcRwLock::new(HashMap::new()),
                proxy_cache_confs: ArcRwLock::new(Vec::with_capacity(10)),
                proxy_cache_index: ArcRwLock::new(ProxyCacheIndex::new()),
                proxy_expires_file_timer: DEFAULT_PROXY_EXPIRES_FILE_TIMER,
                hot_io_percent_map: ArcRwLock::new(HashMap::new()),
                cache_file_node_queue: ArcMutex::new(VecDeque::new()),
                proxy_max_open_file: 0,
                is_init_master_thread: Arc::new(AtomicBool::new(false)),
                session_id,
                tmp_file_id,
            },
        }
    }

    pub fn is_hot_io_percent(&self, name: &String, hot_io_percent: u64) -> bool {
        let value = {
            let value = self.main.hot_io_percent_map.get().get(name).cloned();
            if value.is_none() {
                0
            } else {
                value.unwrap()
            }
        };
        if value >= hot_io_percent {
            return true;
        }
        false
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "proxy_cache".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "proxy_cache_name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache_name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_request_slice".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_request_slice(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_cache_key".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache_key(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_cache_purge".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache_purge(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_cache_methods".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache_methods(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_cache_valid".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_cache_valid(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_expires_file_timer".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_expires_file_timer(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "proxy_hot_file".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_hot_file(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "proxy_max_open_file".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_max_open_file(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "proxy_get_to_get_range".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(proxy_get_to_get_range(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "rewrite".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(rewrite(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER | conf::CMD_CONF_TYPE_LOCAL,
        },
    ]);
}

lazy_static! {
    pub static ref MODULE_FUNC: Arc<module::Func> = Arc::new(module::Func {
        create_conf: |ms| Box::pin(create_conf(ms)),
        merge_conf: |ms, parent_conf, child_conf| Box::pin(merge_conf(ms, parent_conf, child_conf)),

        init_conf: |ms, parent_conf, child_conf| Box::pin(init_conf(ms, parent_conf, child_conf)),
        merge_old_conf: |old_ms, old_main_conf, old_conf, ms, main_conf, conf| Box::pin(
            merge_old_conf(old_ms, old_main_conf, old_conf, ms, main_conf, conf)
        ),
        init_master_thread: Some(
            |ms, parent_conf, child_conf, executor, ms_executor| Box::pin(init_master_thread(
                ms,
                parent_conf,
                child_conf,
                executor,
                ms_executor
            ))
        ),
        init_work_thread: Some(|ms, parent_conf, child_conf, ms_executor| Box::pin(
            init_work_thread(ms, parent_conf, child_conf, ms_executor)
        )),
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_core_proxy".to_string(),
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
    ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    use super::net_core_proxy;
    //当前可能是main  server local， ProxyCache必须到main_conf中读取
    let net_core_proxy = net_core_proxy::main_conf(&ms).await;
    let child_conf = child_conf.get_mut::<Conf>();
    if parent_conf.is_some() {
        let parent_conf = parent_conf.unwrap();
        let parent_conf = parent_conf.get_mut::<Conf>();

        if child_conf.proxy_cache_names.is_none() {
            child_conf.proxy_cache_names = parent_conf.proxy_cache_names.clone();
        }

        if &child_conf.proxy_cache_key == DEFAULT_PROXY_CACHE_KEY {
            child_conf.proxy_cache_key = parent_conf.proxy_cache_key.clone();
        }

        if !child_conf.proxy_cache_purge {
            child_conf.proxy_cache_purge = parent_conf.proxy_cache_purge.clone();
        }

        if child_conf.proxy_request_slice == 1 * CACHE_FILE_SLISE {
            child_conf.proxy_request_slice = parent_conf.proxy_request_slice;
        }

        if child_conf.proxy_cache_methods.is_empty() {
            child_conf.proxy_cache_methods = parent_conf.proxy_cache_methods.clone();
        }

        if child_conf.proxy_cache_valids.is_empty() {
            child_conf.proxy_cache_valids = parent_conf.proxy_cache_valids.clone();
        }

        if child_conf.main.proxy_expires_file_timer == DEFAULT_PROXY_EXPIRES_FILE_TIMER {
            child_conf.main.proxy_expires_file_timer =
                parent_conf.main.proxy_expires_file_timer.clone();
        }

        if !child_conf.proxy_hot_file.is_open {
            child_conf.proxy_hot_file = parent_conf.proxy_hot_file.clone();
        }

        if child_conf.main.proxy_max_open_file == 0 {
            child_conf.main.proxy_max_open_file = parent_conf.main.proxy_max_open_file.clone();
        }

        if child_conf.proxy_get_to_get_range == false {
            child_conf.proxy_get_to_get_range = parent_conf.proxy_get_to_get_range.clone();
        }
    }

    use crate::proxy::stream_var;
    use crate::util::default_config::VAR_STREAM_INFO;
    let ret: Result<Var> = async {
        let vars = Var::new(&child_conf.proxy_cache_key, "")
            .map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
        let mut vars_test = Var::copy(&vars).map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
        vars_test.for_each(|var| {
            let var_name = Var::var_name(var);
            let value = stream_var::find(var_name, &VAR_STREAM_INFO)
                .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
            Ok(value)
        })?;
        let _ = vars_test
            .join()
            .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
        Ok(vars)
    }
    .await;
    let vars = ret.map_err(|e| {
        anyhow!(
            "err:proxy_cache_key => proxy_cache_key:{}, e:{}",
            child_conf.proxy_cache_key,
            e
        )
    })?;
    child_conf.proxy_cache_key_vars = vars;

    if child_conf.proxy_cache_names.is_some() {
        let proxy_cache_names = child_conf.proxy_cache_names.as_ref().unwrap();
        for proxy_cache_name in proxy_cache_names {
            let proxy_cache = net_core_proxy
                .main
                .proxy_cache_map
                .get()
                .get(proxy_cache_name)
                .cloned();
            if proxy_cache.is_none() {
                return Err(anyhow::anyhow!(
                    "err: not find => proxy_cache_name:{}",
                    proxy_cache_name
                ));
            }
            child_conf.proxy_caches.push(proxy_cache.unwrap());
        }
    }

    return Ok(());
}

async fn merge_old_conf(
    _old_ms: Option<module::Modules>,
    _old_main_conf: Option<typ::ArcUnsafeAny>,
    mut old_conf: Option<typ::ArcUnsafeAny>,
    ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    if old_conf.is_some() {
        let old_conf = old_conf.as_mut().unwrap().get_mut::<Conf>();
        conf.main.proxy_cache_index = old_conf.main.proxy_cache_index.clone();
        conf.main.hot_io_percent_map = old_conf.main.hot_io_percent_map.clone();
        conf.main.cache_file_node_queue = old_conf.main.cache_file_node_queue.clone();
        conf.main.is_init_master_thread = old_conf.main.is_init_master_thread.clone();
    }

    use super::net_core_proxy;
    //当前可能是main  server local， ProxyCache必须到main_conf中读取
    let net_core_proxy = net_core_proxy::main_conf(&ms).await;

    let proxy_cache_confs = conf.main.proxy_cache_confs.get().clone();
    for proxy_cache_conf in proxy_cache_confs {
        if !proxy_cache_conf.is_open {
            continue;
        }

        if old_conf.is_some() {
            let old_conf = old_conf.as_mut().unwrap().get_mut::<Conf>();
            let proxy_cache = old_conf
                .main
                .proxy_cache_map
                .get()
                .get(&proxy_cache_conf.name)
                .cloned();
            if proxy_cache.is_some() {
                let proxy_cache = proxy_cache.unwrap();
                conf.main
                    .proxy_cache_map
                    .get_mut()
                    .insert(proxy_cache_conf.name.clone(), proxy_cache);
                continue;
            }
        }

        let proxy_cache = ProxyCache::new(proxy_cache_conf.clone())?;
        let proxy_cache = Arc::new(proxy_cache);
        proxy_cache.load(net_core_proxy).await?;
        conf.main
            .proxy_cache_map
            .get_mut()
            .insert(proxy_cache_conf.name.clone(), proxy_cache);
    }
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

async fn init_master_thread(
    ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
    executor: ExecutorLocalSpawn,
    ms_executor: ExecutorLocalSpawn,
) -> Result<()> {
    let session_id = ms.session_id();
    let conf = conf.get_mut::<Conf>();
    let ret = conf.main.is_init_master_thread.compare_exchange(
        false,
        true,
        Ordering::Acquire,
        Ordering::Relaxed,
    );
    if let Ok(false) = ret {
        executor.clone()._start(
            #[cfg(feature = "anyspawn-count")]
                Some(format!("{}:{}", file!(), line!())),
            move |executors| async move {
                let mut shutdown_thread_rx = executors.context.shutdown_thread_tx.subscribe();
                tokio::select! {
                    biased;
                    _ = timer_check_proxy_cache(ms) => {
                    }
                    _ =  shutdown_thread_rx.recv() => {
                        log::debug!(target: "ms", "ms session_id:{} => init_master_thread exit executor", session_id);
                        return Ok(());
                    }
                    else => {
                        return Err(anyhow!("err:select"));
                    }
                }

                log::trace!(
                    "session_id:{}, init_master_thread net_core_proxy once",
                    session_id
                );
                Ok(())
            },
        );
    }

    ms_executor.clone()._start(
        #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
        move |executors| async move {
            let mut shutdown_thread_rx = executors.context.shutdown_thread_tx.subscribe();
            let interval = 10;
            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {

                    }
                    _ =  shutdown_thread_rx.recv() => {
                        log::debug!(target: "ms", "ms session_id:{} => init_master_thread exit ms_executor", session_id);
                        return Ok(());
                    }
                    else => {
                        return Err(anyhow!("err:select"));
                    }
                }

                log::trace!(
                    "session_id:{}, init_master_thread net_core_proxy",
                    session_id
                );
            }
        },
    );

    return Ok(());
}

async fn init_work_thread(
    ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
    ms_executor: ExecutorLocalSpawn,
) -> Result<()> {
    let session_id = ms.session_id();
    let _conf = conf.get_mut::<Conf>();
    ms_executor.clone()._start(
        #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
        move |executors| async move {
            let mut shutdown_thread_rx = executors.context.shutdown_thread_tx.subscribe();
            let interval = 10;
            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {
                    }
                    _ =  shutdown_thread_rx.recv() => {
                        log::debug!(target: "ms", "ms session_id:{} => init_work_thread exit", session_id);
                        return Ok(());
                    }
                    else => {
                        return Err(anyhow!("err:select"));
                    }
                }

                log::trace!("session_id:{}, init_work_thread net_core_proxy", session_id);
            }
        },
    );
    return Ok(());
}

async fn proxy_cache(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let mut proxy_cache_conf: ProxyCacheConf =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    if proxy_cache_conf.name.len() <= 0 {
        return Err(anyhow!("proxy_cache_conf.name.len() <= 0"));
    }

    if proxy_cache_conf.path.len() <= 0 {
        return Err(anyhow!("proxy_cache_conf.name.len() <= 0"));
    }

    if proxy_cache_conf.path.as_bytes()[proxy_cache_conf.path.len() - 1] != b'/' {
        proxy_cache_conf.path = proxy_cache_conf.path + "/";
    }

    if proxy_cache_conf.levels.len() <= 0 {
        return Err(anyhow!("proxy_cache_conf.name.len() <= 0"));
    }
    if !Path::new(&proxy_cache_conf.path).exists() {
        std::fs::create_dir_all(&proxy_cache_conf.path)
            .map_err(|e| anyhow!("err:create_dir_all => e:{}", e))?;
    }

    c.main.proxy_cache_confs.get_mut().push(proxy_cache_conf);
    log::trace!(target: "main", "c.main.proxy_cache_confs:{:?}", &*c.main.proxy_cache_confs.get());
    return Ok(());
}

async fn proxy_cache_name(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_cache_names = Some(conf_arg.value.get::<Vec<String>>().clone());

    log::trace!(target: "main", "c.proxy_cache_names:{:?}", c.proxy_cache_names);
    return Ok(());
}

async fn proxy_cache_key(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_cache_key = conf_arg.value.get::<String>().clone();

    if c.proxy_cache_key.len() <= 0 {
        return Err(anyhow!("err:c.proxy_cache_key.len() <= 0"));
    }

    use crate::proxy::stream_var;
    use crate::util::default_config::VAR_STREAM_INFO;
    let ret: Result<Var> = async {
        let vars =
            Var::new(&c.proxy_cache_key, "").map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
        let mut vars_test = Var::copy(&vars).map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
        vars_test.for_each(|var| {
            let var_name = Var::var_name(var);
            let value = stream_var::find(var_name, &VAR_STREAM_INFO)
                .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
            Ok(value)
        })?;
        let _ = vars_test
            .join()
            .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
        Ok(vars)
    }
    .await;
    let _vars = ret.map_err(|e| {
        anyhow!(
            "err:proxy_cache_key => proxy_cache_key:{}, e:{}",
            c.proxy_cache_key,
            e
        )
    })?;

    log::trace!(target: "main", "c.proxy_cache_key:{:?}", c.proxy_cache_key);
    return Ok(());
}

async fn proxy_cache_purge(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_cache_purge = conf_arg.value.get::<bool>().clone();

    log::trace!(target: "main", "c.proxy_cache_purge:{:?}", c.proxy_cache_purge);
    return Ok(());
}

async fn proxy_request_slice(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_request_slice = conf_arg.value.get::<usize>().clone() as u64 * CACHE_FILE_SLISE;

    if c.proxy_request_slice <= 0 {
        return Err(anyhow!("err:c.proxy_request_slice <= 0"));
    }

    log::trace!(target: "main", "c.proxy_request_slice:{:?}", c.proxy_request_slice);
    return Ok(());
}

async fn proxy_cache_methods(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let proxy_cache_methods = conf_arg.value.get::<Vec<String>>().clone();

    let mut data = HashMap::new();
    for proxy_cache_method in &proxy_cache_methods {
        let _proxy_cache_method = proxy_cache_method.to_ascii_lowercase();
        if CHECK_METHODS_MAP.get().get(&_proxy_cache_method).is_none() {
            return Err(anyhow::anyhow!(
                "err:{:?} => {} not find",
                proxy_cache_methods,
                proxy_cache_method
            ));
        }
        data.insert(_proxy_cache_method, true);
    }

    if !data.is_empty() {
        c.proxy_cache_methods = data;
        log::trace!(target: "main", "c.proxy_cache_methods:{:?}", c.proxy_cache_methods);
    }
    return Ok(());
}

async fn proxy_cache_valid(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_cache_valid: ProxyCacheValidConf =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;

    let mut data = HashMap::new();
    for proxy_cache_valid in &proxy_cache_valid.proxy_cache_valid {
        for status in &proxy_cache_valid.status {
            let status_num = if status == "any" {
                0
            } else {
                status
                    .parse::<u16>()
                    .map_err(|e| anyhow!("err:str {} => e:{}", str, e))?
            };

            if data.get(&status_num).is_some() {
                return Err(anyhow::anyhow!("err:status {} exist", status));
            }

            data.insert(status_num, proxy_cache_valid.time);
        }
    }

    if !data.is_empty() {
        c.proxy_cache_valids = data;
        log::trace!(target: "main", "c.proxy_cache_valids:{:?}", c.proxy_cache_valids);
    }
    return Ok(());
}

async fn proxy_expires_file_timer(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let proxy_expires_file_timer = conf_arg.value.get::<u64>().clone();
    if proxy_expires_file_timer > 0 {
        c.main.proxy_expires_file_timer = proxy_expires_file_timer;
        log::trace!(target: "main", "c.main.proxy_expires_file_timer:{:?}", c.main.proxy_expires_file_timer);
    }
    return Ok(());
}

async fn proxy_hot_file(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>();
    let proxy_hot_file: ProxyHotFile =
        toml::from_str(str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;
    c.proxy_hot_file = proxy_hot_file;
    log::trace!(target: "main", "c.proxy_hot_file:{:?}", c.proxy_hot_file);
    return Ok(());
}

async fn proxy_max_open_file(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.main.proxy_max_open_file = conf_arg.value.get::<usize>().clone();
    log::trace!(target: "main", "c.main.proxy_max_open_file:{:?}", c.main.proxy_max_open_file);
    return Ok(());
}

async fn proxy_get_to_get_range(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_get_to_get_range = conf_arg.value.get::<bool>().clone();
    log::trace!(target: "main", "c.proxy_get_to_get_range:{:?}", c.proxy_get_to_get_range);
    return Ok(());
}

async fn rewrite(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let strs = conf_arg.value.get::<Vec<String>>().clone();
    if strs.len() != 3 {
        return Err(anyhow::anyhow!("strs.len() != 3"));
    }

    let regex = regex::Regex::new(&strs[0]).map_err(|e| anyhow!("err:Regex::new => e:{}", e))?;
    let vars = Var::new(&strs[1], "").map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
    let status = if strs[2] == "redirect" {
        http::StatusCode::FOUND
    } else if strs[2] == "permanent" {
        http::StatusCode::MOVED_PERMANENTLY
    } else {
        return Err(anyhow::anyhow!("redirect or permanent"));
    };

    let rewrite = ProxyRewrite {
        regex,
        vars,
        status,
    };
    c.rewrite.push(rewrite);

    //log::trace!(target: "main", "rewrite:{:?}", rewrite);
    return Ok(());
}
