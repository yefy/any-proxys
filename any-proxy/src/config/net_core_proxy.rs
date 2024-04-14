use crate::config as conf;
use crate::proxy::http_proxy::http_cache_file::ProxyCache;
use crate::util::var::Var;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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

pub const CACHE_FILE_SLISE: u64 = 1024 * 1024;
const DEFAULT_PROXY_CACHE_KEY: &str = "${http_ups_request_scheme}${http_ups_request_method}${http_ups_request_host}${http_ups_request_uri}";

pub struct Conf {
    pub proxy_cache_map: HashMap<String, Arc<ProxyCache>>,
    pub proxy_caches: Vec<Arc<ProxyCache>>,
    pub proxy_cache_confs: Vec<ProxyCacheConf>,
    pub proxy_cache_names: Vec<String>,
    pub proxy_request_slice: u64,
    pub proxy_cache_key: String,
    pub proxy_cache_key_vars: Var,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            proxy_cache_map: HashMap::new(),
            proxy_caches: Vec::with_capacity(10),
            proxy_cache_confs: Vec::with_capacity(10),
            proxy_cache_names: Vec::with_capacity(10),
            proxy_request_slice: 1 * CACHE_FILE_SLISE,
            proxy_cache_key: DEFAULT_PROXY_CACHE_KEY.to_string(),
            proxy_cache_key_vars: Var::new(DEFAULT_PROXY_CACHE_KEY, "").unwrap(),
        }
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

        if child_conf.proxy_cache_names.is_empty() {
            child_conf.proxy_cache_names = parent_conf.proxy_cache_names.clone();
        }

        if &child_conf.proxy_cache_key == DEFAULT_PROXY_CACHE_KEY {
            child_conf.proxy_cache_key = parent_conf.proxy_cache_key.clone();
        }

        if child_conf.proxy_request_slice == 1 * CACHE_FILE_SLISE {
            child_conf.proxy_request_slice = parent_conf.proxy_request_slice;
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

    for proxy_cache_name in &child_conf.proxy_cache_names {
        let proxy_cache = net_core_proxy
            .proxy_cache_map
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

    return Ok(());
}

async fn merge_old_conf(
    _old_ms: Option<module::Modules>,
    _old_main_conf: Option<typ::ArcUnsafeAny>,
    mut old_conf: Option<typ::ArcUnsafeAny>,
    _ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let conf = conf.get_mut::<Conf>();
    for proxy_cache_conf in &conf.proxy_cache_confs {
        if !proxy_cache_conf.is_open {
            continue;
        }

        if old_conf.is_some() {
            let old_conf = old_conf.as_mut().unwrap().get_mut::<Conf>();
            let proxy_cache = old_conf
                .proxy_cache_map
                .get(&proxy_cache_conf.name)
                .cloned();
            if proxy_cache.is_none() {
                continue;
            }
            let proxy_cache = proxy_cache.unwrap();
            conf.proxy_cache_map
                .insert(proxy_cache_conf.name.clone(), proxy_cache);
            continue;
        }

        let mut proxy_cache = ProxyCache::new(proxy_cache_conf.clone())?;
        proxy_cache.load().await?;
        conf.proxy_cache_map
            .insert(proxy_cache_conf.name.clone(), Arc::new(proxy_cache));
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

    c.proxy_cache_confs.push(proxy_cache_conf);
    log::trace!(target: "main", "c.proxy_cache_confs:{:?}", c.proxy_cache_confs);
    return Ok(());
}

async fn proxy_cache_name(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.proxy_cache_names = conf_arg.value.get::<Vec<String>>().clone();

    if c.proxy_cache_names.len() <= 0 {
        return Err(anyhow!("err:c.proxy_cache_names.len() <= 0"));
    }

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
        return Err(anyhow!("err:c.proxy_cache_names.len() <= 0"));
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
