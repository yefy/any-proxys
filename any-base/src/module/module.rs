use crate::executor_local_spawn::ExecutorLocalSpawn;
use crate::typ::{ArcMutex, ArcRwLock, ArcUnsafeAny};
use crate::DropMsExecutor;
use anyhow::anyhow;
use anyhow::Result;
use async_recursion::async_recursion;
use async_trait::async_trait;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/*
//主模块才有
create_server

//创建main
create_main_confs -> create_conf

//创建server
create_server_confs -> create_conf

//创建local
create_local_confs -> create_conf

for {
      main -> server -> local
}
//上面基本只能做自己的配置文件解析


//相同main中有个多个配置要处理，可以在自己的main中处理
//不同的main中有个多个配置要处理，可以统一一个相同main中处理
// main -> server -> local做完，才开始初始化
//只是初始化全部main
init_main_confs_map   ->   init_conf

//共享数据引用在这里处理， 共享数据只能保存在main中
// main -> server -> local做完，才开始merge
//只是old main， main全部main合并
merge_old_main_confs_map  -> merge_old_conf

//server local 要引用main中的数据， 先合并完后在访问上级
// main -> server -> local做完，才开始merge
//递归main  server local  进行合并
merge_confs_map    -> merge_conf

//全局共享数据参考 any-proxy\src\confighttp_core_proxy.rs   proxy_cache  proxy_cache_name的处理
*/

lazy_static! {
    pub static ref SESSION_ID: Arc<AtomicU64> = Arc::new(AtomicU64::new(100000));
}

pub fn get_session_id() -> u64 {
    SESSION_ID.fetch_add(1, Ordering::Relaxed)
}

lazy_static! {
    pub static ref G_MODULES: ArcMutex<GModules> = ArcMutex::new(GModules::new());
}

pub fn get_modules() -> ArcMutex<GModules> {
    return G_MODULES.clone();
}

pub fn add_module(module: ArcRwLock<Module>) -> Result<()> {
    get_modules().get_mut().add_module(module)
}

pub fn parse_modules() -> Result<()> {
    get_modules().get_mut().parse_modules()
}

#[derive(Clone)]
pub struct GModules {
    modules: Vec<ArcRwLock<Module>>,
    main_map: HashMap<String, ArcRwLock<Module>>,
    main_index_len: i32,
}

impl GModules {
    pub fn new() -> GModules {
        GModules {
            modules: Vec::new(),
            main_map: HashMap::new(),
            main_index_len: -1,
        }
    }

    pub fn get_modules(&self) -> &Vec<ArcRwLock<Module>> {
        &self.modules
    }

    pub fn main_index_len(&self) -> usize {
        if self.main_index_len <= 0 {
            panic!("main_index_len:{}", self.main_index_len)
        }
        self.main_index_len as usize
    }

    pub fn get_module(&mut self, str: &str) -> ArcRwLock<Module> {
        self.main_map.get(str).cloned().unwrap()
    }

    pub fn add_module(&mut self, module: ArcRwLock<Module>) -> Result<()> {
        if self.main_map.get(&module.get().name).is_some() {
            return Err(anyhow::anyhow!("err:{} is exist", module.get().name));
        }
        self.main_map
            .insert(module.get().name.clone(), module.clone());
        self.modules.push(module);
        Ok(())
    }

    pub fn get_module_servers(&mut self, value: ArcUnsafeAny) -> Result<Vec<Box<dyn Server>>> {
        let mut module_servers = Vec::new();
        for module in self.get_modules() {
            let module_ = &*module.get();

            if module_.create_server.is_none() {
                continue;
            }
            let module_server = (module_.create_server.as_ref().unwrap())(value.clone())?;
            module_servers.push(module_server)
        }
        return Ok(module_servers);
    }

    pub fn parse_modules(&mut self) -> Result<()> {
        let mut index: i32 = 0;
        let mut main_index: i32 = 0;
        let mut main_names = HashMap::new();

        for module in self.get_modules() {
            module.get_mut().index = index;
            index += 1;

            if module.get().create_main_confs.is_none() {
                continue;
            }
            let typ = module.get().typ;
            let name = module.get().name.clone();
            let main_name = main_names.get(&typ);
            if main_name.is_some() {
                panic!("typ:{}, {}, {}", typ, main_name.unwrap(), name)
            }
            main_names.insert(typ, name.clone());

            module.get_mut().main_index = main_index;

            let mut ctx_index: i32 = 0;
            for sub_module in self.get_modules() {
                if sub_module.get().typ & typ == 0 {
                    continue;
                }
                sub_module.get_mut().ctx_index = ctx_index;
                ctx_index += 1;
            }

            log::info!(
                "name:{:?}, typ:{:?}, main_index:{:?}, ctx_index_len:{:?}",
                name,
                typ,
                main_index,
                ctx_index
            );

            for sub_module in self.get_modules() {
                if sub_module.get().typ & typ == 0 {
                    continue;
                }
                log::info!(
                    "    name:{:?}, typ:{:?}, main_index:{:?}, ctx_index_len:{:?}",
                    sub_module.get().name,
                    typ,
                    main_index,
                    sub_module.get().ctx_index
                );
                sub_module.get_mut().main_index = main_index;
                sub_module.get_mut().ctx_index_len = ctx_index;
            }

            main_index += 1;
        }
        log::info!("index:{:?}", index);
        log::info!("main_index_len:{:?}", main_index);
        if main_index == 0 {
            return Err(anyhow!("err: main module is nil"));
        }
        self.main_index_len = main_index;
        return Ok(());
    }
}

/*
r###
#afafa
include  =  fafafafa;
a u64 = 123;#afafafafafa
a str = 123;
a strs = 123 1233;
a time = 2013:3e22:232;
a raw = r```fafaffafa
a bool = true;
fafafa
fafhaofa
faofjaof
fafa
```r   ; #fafafafafaf
###r
*/

#[async_trait]
pub trait Server {
    async fn start(&mut self, ms: Modules, any: ArcUnsafeAny) -> Result<()>;
    async fn send(&self, any: ArcUnsafeAny) -> Result<()>;
    async fn wait(&self, any: ArcUnsafeAny) -> Result<()>;
    async fn stop(&self, any: ArcUnsafeAny) -> Result<()>;
}

#[derive(Clone)]
pub struct ConfArg {
    pub is_sub_file: bool,
    pub file_name: String,
    pub path: String,
    pub module_type: Arc<AtomicUsize>,
    pub cmd_conf_type: Arc<AtomicUsize>,
    pub main_index: Arc<AtomicI32>,
    pub reader: ArcMutex<BufReader<fs::File>>,
    pub line_num: Arc<AtomicUsize>,
    pub is_block: bool,
    pub any: ArcUnsafeAny,
    pub parent_module_confs: Vec<ArcUnsafeAny>,
    pub last_module_confs: ArcUnsafeAny,
    pub curr_module_confs: ArcUnsafeAny,

    //临时用的
    pub str_raw: String,
    pub str_key: String,
    pub str_typ: String,
    pub str_value: String,
    pub value: ArcUnsafeAny,
}

impl ConfArg {
    pub fn new() -> ConfArg {
        ConfArg {
            is_sub_file: false,
            file_name: "".to_string(),
            path: "".to_string(),
            module_type: Arc::new(AtomicUsize::new(0)),
            cmd_conf_type: Arc::new(AtomicUsize::new(0)),
            main_index: Arc::new(AtomicI32::new(-1)),
            str_raw: "".to_string(),
            str_key: "".to_string(),
            str_typ: "".to_string(),
            str_value: "".to_string(),
            reader: ArcMutex::default(),
            line_num: Arc::new(AtomicUsize::new(0)),
            value: ArcUnsafeAny::default(),
            is_block: false,
            any: ArcUnsafeAny::default(),
            last_module_confs: ArcUnsafeAny::default(),
            curr_module_confs: ArcUnsafeAny::default(),
            parent_module_confs: Vec::new(),
        }
    }
    pub fn last_conf(&self) -> &ArcUnsafeAny {
        &self.last_module_confs
    }
    pub fn curr_conf(&self) -> &ArcUnsafeAny {
        &self.curr_module_confs
    }
}

const BLOCK_COMMENTS_START: &'static str = "r###";
const BLOCK_COMMENTS_END: &'static str = "###r";
const LINE_COMMENTS: &'static str = "#";
const LINE_END: &'static str = ";";
const BLOCK_START: &'static str = "r```";
const BLOCK_END: &'static str = "```r";
const KEY_VALUE_DELIMITER: &'static str = "=";
const MAIN_START: &'static str = "{";
const MAIN_END: &'static str = "}";

pub const CMD_TYPE_BOOL: &'static str = "bool";

pub const CMD_TYPE_I8: &'static str = "i8";
pub const CMD_TYPE_I16: &'static str = "i16";
pub const CMD_TYPE_I32: &'static str = "i32";
pub const CMD_TYPE_ISIZE: &'static str = "isize";
pub const CMD_TYPE_I64: &'static str = "i64";

pub const CMD_TYPE_I8S: &'static str = "i8s";
pub const CMD_TYPE_I16S: &'static str = "i16s";
pub const CMD_TYPE_I32S: &'static str = "i32s";
pub const CMD_TYPE_ISIZES: &'static str = "isizes";
pub const CMD_TYPE_I64S: &'static str = "i64s";

pub const CMD_TYPE_U8: &'static str = "u8";
pub const CMD_TYPE_U16: &'static str = "u16";
pub const CMD_TYPE_U32: &'static str = "u32";
pub const CMD_TYPE_USIZE: &'static str = "usize";
pub const CMD_TYPE_U64: &'static str = "u64";

pub const CMD_TYPE_U8S: &'static str = "u8s";
pub const CMD_TYPE_U16S: &'static str = "u16s";
pub const CMD_TYPE_U32S: &'static str = "u32s";
pub const CMD_TYPE_USIZES: &'static str = "usizes";
pub const CMD_TYPE_U64S: &'static str = "u64s";

pub const CMD_TYPE_F64: &'static str = "f64";
pub const CMD_TYPE_F64S: &'static str = "f64s";

pub const CMD_TYPE_STR: &'static str = "str";
pub const CMD_TYPE_STRS: &'static str = "strs";
pub const CMD_TYPE_RAW: &'static str = "raw";

lazy_static! {
    static ref VALUE_MAP: HashMap<String, SetConfArgValue> = {
        let mut map: HashMap<String, SetConfArgValue> = HashMap::new();
        map.insert(CMD_TYPE_BOOL.to_string(), bool_value);

        map.insert(CMD_TYPE_I8.to_string(), i8_value);
        map.insert(CMD_TYPE_I16.to_string(), i16_value);
        map.insert(CMD_TYPE_I32.to_string(), i32_value);
        map.insert(CMD_TYPE_ISIZE.to_string(), isize_value);
        map.insert(CMD_TYPE_I64.to_string(), i64_value);

        map.insert(CMD_TYPE_U8.to_string(), u8_value);
        map.insert(CMD_TYPE_U16.to_string(), u16_value);
        map.insert(CMD_TYPE_U32.to_string(), u32_value);
        map.insert(CMD_TYPE_USIZE.to_string(), usize_value);
        map.insert(CMD_TYPE_U64.to_string(), u64_value);

        map.insert(CMD_TYPE_F64.to_string(), f64_value);
        map.insert(CMD_TYPE_STR.to_string(), str_value);

        map.insert(CMD_TYPE_I8S.to_string(), i8s_value);
        map.insert(CMD_TYPE_I16S.to_string(), i16s_value);
        map.insert(CMD_TYPE_I32S.to_string(), i32s_value);
        map.insert(CMD_TYPE_ISIZES.to_string(), isizes_value);
        map.insert(CMD_TYPE_I64S.to_string(), i64s_value);

        map.insert(CMD_TYPE_U8S.to_string(), u8s_value);
        map.insert(CMD_TYPE_U16S.to_string(), u16s_value);
        map.insert(CMD_TYPE_U32S.to_string(), u32s_value);
        map.insert(CMD_TYPE_USIZES.to_string(), usizes_value);
        map.insert(CMD_TYPE_U64S.to_string(), u64s_value);

        map.insert(CMD_TYPE_F64S.to_string(), f64s_value);
        map.insert(CMD_TYPE_STRS.to_string(), strs_value);
        map.insert(CMD_TYPE_RAW.to_string(), str_value);

        map
    };
}

fn bool_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    if value == "true" {
        conf_arg.value.set(Box::new(true));
    } else {
        conf_arg.value.set(Box::new(false));
    }
    return Ok(());
}

fn i8_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<i8>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn i16_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<i16>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn i32_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<i32>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn isize_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<isize>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn i64_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<i64>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn u8_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<u8>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn u16_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<u16>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn u32_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<u32>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn usize_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<usize>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn u64_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<u64>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn f64_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let v = value.parse::<f64>()?;
    conf_arg.value.set(Box::new(v));
    return Ok(());
}

fn trim_str_value(value: &str) -> &str {
    let value = if value == "" || value == "\"" || value == "\"\"" || value.len() <= 0 {
        ""
    } else {
        let value = if &value[0..1] == "\"" {
            &value[1..]
        } else {
            &value[0..]
        };

        let len = value.len();
        let value = if &value[len - 1..] == "\"" {
            &value[0..len - 1]
        } else {
            &value[0..]
        };
        value
    };
    value
}
fn str_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let value = trim_str_value(value);
    conf_arg.value.set(Box::new(value.to_string()));
    return Ok(());
}

fn i8s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<i8>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn i16s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<i16>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn i32s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<i32>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn isizes_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<isize>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn i64s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<i64>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn u8s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<u8>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn u16s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<u16>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn u32s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<u32>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn usizes_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<usize>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn u64s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<u64>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn f64s_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = v.parse::<f64>()?;
        vs.push(v);
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

fn strs_value(conf_arg: &mut ConfArg, value: &str) -> Result<()> {
    let value = trim_str_value(value);
    let mut vs = Vec::new();
    let values = value.split(" ").collect::<Vec<_>>();
    for v in values {
        let v = v.trim();
        if v.len() <= 0 {
            continue;
        }
        let v = trim_str_value(v);
        vs.push(v.to_string());
    }

    conf_arg.value.set(Box::new(vs));
    return Ok(());
}

type Set = fn(Modules, ConfArg, Cmd, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>>>>;
type CreateConf = fn(Modules) -> Pin<Box<dyn Future<Output = Result<ArcUnsafeAny>>>>;
type MergeConf = fn(
    Modules,
    Option<ArcUnsafeAny>,
    ArcUnsafeAny,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type InitConf =
    fn(Modules, ArcUnsafeAny, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type MergeOldConf = fn(
    Option<Modules>,
    Option<ArcUnsafeAny>,
    Option<ArcUnsafeAny>,
    Modules,
    ArcUnsafeAny,
    ArcUnsafeAny,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type CreateMainConfs = fn(Modules) -> Pin<Box<dyn Future<Output = Result<ArcUnsafeAny>>>>;
type InitMainConfs = fn(Modules, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type MergeOldMainConfs = fn(
    Option<Modules>,
    Option<ArcUnsafeAny>,
    Modules,
    ArcUnsafeAny,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type DropConf =
    fn(Modules, ArcUnsafeAny, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type MergeConfs = fn(Modules, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type DropConfs = fn(Modules, ArcUnsafeAny) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type InitMasterThread = fn(
    Modules,
    ArcUnsafeAny,
    ArcUnsafeAny,
    ExecutorLocalSpawn,
    ExecutorLocalSpawn,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type InitWorkThread = fn(
    Modules,
    ArcUnsafeAny,
    ArcUnsafeAny,
    ExecutorLocalSpawn,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type InitMasterThreadConfs = fn(
    Modules,
    ArcUnsafeAny,
    ExecutorLocalSpawn,
    ExecutorLocalSpawn,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type InitWorkThreadConfs = fn(
    Modules,
    ArcUnsafeAny,
    ExecutorLocalSpawn,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

type CreateServer = fn(ArcUnsafeAny) -> Result<Box<dyn Server>>;
type SetConfArgValue = fn(&mut ConfArg, &str) -> Result<()>;
pub fn ret_err(conf_arg: &mut ConfArg, str: &str) -> Result<()> {
    return Err(anyhow!(
        "err:{} -> {}:{}:{}:{}",
        str,
        conf_arg.path,
        conf_arg.file_name,
        conf_arg.line_num.load(Ordering::Relaxed),
        conf_arg.str_raw
    ));
}

pub fn ret_errs(ret: Result<()>, conf_arg: &mut ConfArg, str: &str) -> Result<()> {
    match ret {
        Ok(t) => Ok(t),
        Err(e) => {
            return Err(anyhow!(
                "err:{} -> {}:{}:{}:{} => {}",
                str,
                conf_arg.path,
                conf_arg.file_name,
                conf_arg.line_num.load(Ordering::Relaxed),
                conf_arg.str_raw,
                e
            ));
        }
    }
}

pub fn ret_err_from_err(conf_arg: &mut ConfArg, str: &str) -> Result<()> {
    return Err(anyhow!(
        "err:{} -> {}:{}:{}:{}",
        str,
        conf_arg.path,
        conf_arg.file_name,
        conf_arg.line_num.load(Ordering::Relaxed),
        conf_arg.str_raw
    ));
}

type CmdType = usize;
pub const CMD_TYPE_MAIN: CmdType = 0;
pub const CMD_TYPE_SUB: CmdType = 1;
pub const CMD_TYPE_DATA: CmdType = 2;
pub const CMD_TYPE_BLOCK: CmdType = 3;

#[derive(Clone)]
pub struct Cmd {
    pub name: String,
    pub set: Set,
    pub typ: CmdType,
    pub conf_typ: usize,
}

#[derive(Debug, Clone)]
pub struct Func {
    pub create_conf: CreateConf,
    pub merge_conf: MergeConf,
    pub init_conf: InitConf,
    pub merge_old_conf: MergeOldConf,
    pub init_master_thread: Option<InitMasterThread>,
    pub init_work_thread: Option<InitWorkThread>,
    pub drop_conf: Option<DropConf>,
}

#[derive(Clone)]
pub struct Module {
    pub name: String,
    pub main_index: i32,
    pub ctx_index: i32,
    pub index: i32,
    pub ctx_index_len: i32,
    pub func: Arc<Func>,
    pub cmds: Arc<Vec<Cmd>>,
    pub create_main_confs: Option<CreateMainConfs>,
    pub init_main_confs: Option<InitMainConfs>,
    pub merge_old_main_confs: Option<MergeOldMainConfs>,
    pub merge_confs: Option<MergeConfs>,
    pub init_master_thread_confs: Option<InitMasterThreadConfs>,
    pub init_work_thread_confs: Option<InitWorkThreadConfs>,
    pub drop_confs: Option<DropConfs>,
    pub typ: usize,
    pub create_server: Option<CreateServer>,
}

#[derive(Clone)]
pub struct ModuleDropTest {
    session_id: u64,
}

impl ModuleDropTest {
    pub fn new(session_id: u64) -> Self {
        ModuleDropTest { session_id }
    }
}

impl Drop for ModuleDropTest {
    fn drop(&mut self) {
        log::debug!(target: "ms", "ms session_id:{}, drop ModuleDropTest", self.session_id);
    }
}

#[derive(Clone)]
pub struct Modules {
    session_id: u64,
    main_confs: ArcUnsafeAny,
    init_main_confs_map: ArcMutex<HashMap<i32, (String, InitMainConfs)>>,
    is_finish: Arc<AtomicBool>,
    main_index: Arc<AtomicI32>,
    old_ms: ArcMutex<Modules>,
    merge_old_main_confs_map: ArcMutex<HashMap<i32, (String, MergeOldMainConfs)>>,
    merge_confs_map: ArcMutex<HashMap<i32, (String, MergeConfs)>>,
    main_indexs: ArcMutex<Vec<i32>>,
    init_master_thread_confs_map: ArcMutex<HashMap<i32, (String, InitMasterThreadConfs)>>,
    init_work_thread_confs_map: ArcMutex<HashMap<i32, (String, InitWorkThreadConfs)>>,
    pub drop_confs_map: ArcMutex<HashMap<i32, (String, DropConfs)>>,
    is_work_thread: bool,
    curr_module_confs: ArcUnsafeAny,
    cmd_conf_type: usize,
    _drop_test: Arc<ModuleDropTest>,
    pub drop_ms_executor: ArcMutex<DropMsExecutor>,
    pub ms_tx: Option<async_channel::Sender<Modules>>,
}

impl Drop for Modules {
    fn drop(&mut self) {
        log::trace!(target: "ms", "drop modules session_id:{}, count:{}", self.session_id, Arc::strong_count(&self._drop_test));
        if Arc::strong_count(&self._drop_test) == 1 {
            if self.ms_tx.is_some() {
                let ms_tx = self.ms_tx.take().unwrap();
                let _ = ms_tx.send_blocking(self.clone());
            }
        }
    }
}

unsafe impl Send for Modules {}
impl Modules {
    pub fn count(&self) -> usize {
        Arc::strong_count(&self._drop_test)
    }
    pub fn curr_conf(&self) -> &ArcUnsafeAny {
        &self.curr_module_confs
    }

    pub fn set_cmd_conf_type(&mut self, cmd_conf_type: usize) {
        self.cmd_conf_type = cmd_conf_type;
    }

    pub fn cmd_conf_type(&self) -> usize {
        self.cmd_conf_type
    }

    pub fn new(old_ms: Option<Modules>, is_work_thread: bool) -> Modules {
        let (old_ms, ms_tx) = if old_ms.is_some() {
            let old_ms = old_ms.unwrap();
            let ms_tx = old_ms.ms_tx.clone();
            (ArcMutex::new(old_ms), ms_tx)
        } else {
            (ArcMutex::default(), None.into())
        };

        let session_id = get_session_id();
        Modules {
            session_id,
            main_confs: ArcUnsafeAny::default(),
            init_main_confs_map: ArcMutex::default(),
            is_finish: Arc::new(AtomicBool::new(false)),
            main_index: Arc::new(AtomicI32::new(-1)),
            old_ms,
            merge_old_main_confs_map: ArcMutex::default(),
            merge_confs_map: ArcMutex::default(),
            main_indexs: ArcMutex::default(),
            init_master_thread_confs_map: ArcMutex::default(),
            init_work_thread_confs_map: ArcMutex::default(),
            drop_confs_map: ArcMutex::default(),
            is_work_thread,
            curr_module_confs: ArcUnsafeAny::default(),
            cmd_conf_type: 0,
            _drop_test: Arc::new(ModuleDropTest::new(session_id)),
            drop_ms_executor: ArcMutex::default(),
            ms_tx,
        }
    }

    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    pub fn is_work_thread(&self) -> bool {
        self.is_work_thread
    }

    pub fn main_confs(&self) -> ArcUnsafeAny {
        self.main_confs.clone()
    }

    pub fn get_modules(&self) -> Vec<ArcRwLock<Module>> {
        G_MODULES.get().get_modules().clone()
    }

    pub fn get_module_servers(&mut self, value: ArcUnsafeAny) -> Result<Vec<Box<dyn Server>>> {
        G_MODULES.get_mut().get_module_servers(value)
    }

    pub fn main_index_len(&mut self) -> usize {
        G_MODULES.get().main_index_len()
    }

    pub async fn init_master_thread(&self, dmse: DropMsExecutor) -> Result<()> {
        let executor = dmse.executor();
        let ms_executor = dmse.ms_executor();
        self.drop_ms_executor.set(dmse);
        for (_, main_index) in self.main_indexs.get().iter().enumerate() {
            {
                let v = self
                    .init_master_thread_confs_map
                    .get_mut()
                    .remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "init_master_thread_confs_map main_index:{}, name:{}", main_index, name);
                    (v)(
                        self.clone(),
                        self.main_confs.clone(),
                        executor.clone(),
                        ms_executor.clone(),
                    )
                    .await
                    .map_err(|e| anyhow!("err:init_master_thread_confs_map =>e{}", e))?;
                }
            }
        }
        log::debug!(target: "ms", "ms session_id:{}, count:{} => init_master_thread", self.session_id(), self.count());
        return Ok(());
    }

    pub async fn init_work_thread(&self, dmse: DropMsExecutor) -> Result<()> {
        let ms_executor = dmse.ms_executor();
        self.drop_ms_executor.set(dmse);
        let main_indexs = self.main_indexs.get().clone();
        for (_, main_index) in main_indexs.iter().enumerate() {
            {
                let v = self
                    .init_work_thread_confs_map
                    .get_mut()
                    .remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "init_work_thread_confs_map main_index:{}, name:{}", main_index, name);
                    (v)(self.clone(), self.main_confs.clone(), ms_executor.clone())
                        .await
                        .map_err(|e| anyhow!("err:init_work_thread_confs_map =>e{}", e))?;
                }
            }
        }
        log::debug!(target: "ms", "ms session_id:{}, count:{} => init_work_thread", self.session_id(), self.count());
        return Ok(());
    }

    pub async fn drop_confs(
        &self,
        mut drop_confs_map: HashMap<i32, (String, DropConfs)>,
    ) -> Result<()> {
        let main_indexs = self.main_indexs.get().clone();
        for (_, main_index) in main_indexs.iter().enumerate() {
            {
                let v = drop_confs_map.remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "drop_confs main_index:{}, name:{}", main_index, name);
                    (v)(self.clone(), self.main_confs.clone())
                        .await
                        .map_err(|e| anyhow!("err:drop_confs =>e{}", e))?;
                }
            }
        }
        log::debug!(target: "ms", "ms session_id:{}, count:{} => drop_confs", self.session_id(), self.count());
        return Ok(());
    }

    pub async fn parse_module_config(
        &mut self,
        path_full_name: &str,
        any: Option<Box<dyn std::any::Any>>,
    ) -> Result<()> {
        log::trace!(target: "main", "================parse_module_config start================");
        let mut main_confs: Vec<ArcUnsafeAny> = Vec::with_capacity(self.main_index_len());
        let mut init_main_confs_map = HashMap::new();
        let mut merge_old_main_confs_map = HashMap::new();
        let mut merge_confs_map = HashMap::new();
        let mut main_indexs = Vec::with_capacity(100);
        let mut init_master_thread_confs_map = HashMap::new();
        let mut init_work_thread_confs_map = HashMap::new();
        let mut drop_confs_map = HashMap::new();

        for module in self.get_modules() {
            let create_main_confs = { module.get().create_main_confs.clone() };
            if create_main_confs.is_none() {
                continue;
            }
            {
                let module = module.get();
                let init_main_confs = module.init_main_confs.clone();
                if init_main_confs.is_none() {
                    panic!("init_main_confs nil name:{}", module.name)
                }
                main_indexs.push(module.main_index);
                init_main_confs_map.insert(
                    module.main_index,
                    (module.name.clone(), init_main_confs.unwrap()),
                );
            }

            {
                let module = module.get();
                let merge_old_main_confs = module.merge_old_main_confs.clone();
                if merge_old_main_confs.is_none() {
                    panic!("merge_old_main_confs nil name:{}", module.name)
                }
                merge_old_main_confs_map.insert(
                    module.main_index,
                    (module.name.clone(), merge_old_main_confs.unwrap()),
                );
            }

            {
                let module = module.get();
                let merge_confs = module.merge_confs.clone();
                if merge_confs.is_some() {
                    merge_confs_map.insert(
                        module.main_index,
                        (module.name.clone(), merge_confs.unwrap()),
                    );
                }
            }

            {
                let module = module.get();
                let init_master_thread_confs = module.init_master_thread_confs.clone();
                if init_master_thread_confs.is_some() {
                    init_master_thread_confs_map.insert(
                        module.main_index,
                        (module.name.clone(), init_master_thread_confs.unwrap()),
                    );
                }
            }

            {
                let module = module.get();
                let init_work_thread_confs = module.init_work_thread_confs.clone();
                if init_work_thread_confs.is_some() {
                    init_work_thread_confs_map.insert(
                        module.main_index,
                        (module.name.clone(), init_work_thread_confs.unwrap()),
                    );
                }
            }

            {
                let module = module.get();
                let drop_confs = module.drop_confs.clone();
                if drop_confs.is_some() {
                    drop_confs_map.insert(
                        module.main_index,
                        (module.name.clone(), drop_confs.unwrap()),
                    );
                }
            }

            log::trace!(target: "main", "parse_module_config module name:{}", module.get().name);
            let conf = (create_main_confs.unwrap())(self.clone()).await?;
            main_confs.push(conf);
        }
        self.init_main_confs_map = ArcMutex::new(init_main_confs_map);
        self.merge_old_main_confs_map = ArcMutex::new(merge_old_main_confs_map);
        self.merge_confs_map = ArcMutex::new(merge_confs_map);
        self.main_indexs = ArcMutex::new(main_indexs);
        self.init_master_thread_confs_map = ArcMutex::new(init_master_thread_confs_map);
        self.init_work_thread_confs_map = ArcMutex::new(init_work_thread_confs_map);
        self.drop_confs_map = ArcMutex::new(drop_confs_map);
        self.main_confs = ArcUnsafeAny::new(Box::new(main_confs));

        let mut conf_arg = ConfArg::new();
        if any.is_some() {
            conf_arg.any.set(any.unwrap());
        }

        self.parse_config_file(&mut conf_arg, path_full_name, self.main_confs.clone())
            .await?;

        let keys = self
            .init_main_confs_map
            .get()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<i32>>();

        for (_, main_index) in keys.iter().enumerate() {
            {
                let v = self.init_main_confs_map.get_mut().remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "init_main_confs_map main_index:{}, name:{}", main_index, name);
                    (v)(self.clone(), self.main_confs.clone())
                        .await
                        .map_err(|e| anyhow!("err:init_main_confs_map =>e{}", e))?;
                }
            }
            {
                let old_ms = self.old_ms.get_option().clone();
                let v = self.merge_old_main_confs_map.get_mut().remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "merge_old_main_confs_map main_index:{}, name:{}", main_index, name);
                    let old_main_conf = if old_ms.is_some() {
                        Some(old_ms.as_ref().unwrap().main_confs())
                    } else {
                        None
                    };
                    (v)(old_ms, old_main_conf, self.clone(), self.main_confs())
                        .await
                        .map_err(|e| anyhow!("err:merge_old_main_confs_map =>e{}", e))?;
                }
            }

            {
                let v = self.merge_confs_map.get_mut().remove(&main_index);
                if v.is_some() {
                    let (name, v) = v.unwrap();
                    log::trace!(target: "main", "merge_confs_map main_index:{}, name:{}", main_index, name);
                    (v)(self.clone(), self.main_confs.clone())
                        .await
                        .map_err(|e| anyhow!("err:merge_confs_map =>e{}", e))?;
                }
            }
        }

        if self.old_ms.is_some() {
            self.old_ms.set_nil();
        }
        self.is_finish.store(true, Ordering::SeqCst);

        log::debug!(target: "ms", "ms session_id:{}, count:{} => parse_module_config", self.session_id(), self.count());
        log::trace!(target: "main", "================parse_module_config end================");
        return Ok(());
    }

    pub async fn get_main_conf<T: 'static>(&self, module: ArcRwLock<Module>) -> &T {
        if !self.is_finish.load(Ordering::SeqCst) {
            let main_index = module.get().main_index;
            let self_main_index = self.main_index.load(Ordering::SeqCst);
            if main_index != self_main_index {
                log::trace!(target: "main",
                    "self_main_index:{}, main_index:{}",
                    self_main_index,
                    main_index
                );
                {
                    let v = self.init_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "init_main_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:init_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }

                {
                    let old_ms = self.old_ms.get_option().clone();
                    let v = self.merge_old_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_old_main_confs_map main_index:{}, name:{}", main_index, name);
                        let old_main_conf = if old_ms.is_some() {
                            Some(old_ms.as_ref().unwrap().main_confs())
                        } else {
                            None
                        };
                        (v)(old_ms, old_main_conf, self.clone(), self.main_confs())
                            .await
                            .map_err(|e| anyhow!("err:merge_old_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }

                {
                    let v = self.merge_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:merge_confs_map =>e{}", e))
                            .unwrap();
                    }
                }
            }
        }
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        if self.main_confs.is_none() {
            panic!(
                "err:when modules are initialized, they cannot access each other => module.name:{}",
                module.name
            )
        }
        let main_confs = self.main_confs.get::<Vec<ArcUnsafeAny>>();
        let main_conf = &main_confs[module.main_index as usize];
        let module_confs = main_conf.get::<Vec<ArcUnsafeAny>>();
        let module_conf = &module_confs[module.ctx_index as usize];
        module_conf.get::<T>()
    }

    pub async fn get_main_conf_mut<T: 'static>(&self, module: ArcRwLock<Module>) -> &mut T {
        if !self.is_finish.load(Ordering::SeqCst) {
            let main_index = module.get().main_index;
            let self_main_index = self.main_index.load(Ordering::SeqCst);
            if main_index != self_main_index {
                log::trace!(target: "main",
                    "self_main_index:{}, main_index:{}",
                    self_main_index,
                    main_index
                );
                {
                    let v = self.init_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "init_main_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:init_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }
                {
                    let old_ms = self.old_ms.get_option().clone();
                    let v = self.merge_old_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_old_main_confs_map main_index:{}, name:{}", main_index, name);
                        let old_main_conf = if old_ms.is_some() {
                            Some(old_ms.as_ref().unwrap().main_confs())
                        } else {
                            None
                        };
                        (v)(old_ms, old_main_conf, self.clone(), self.main_confs())
                            .await
                            .map_err(|e| anyhow!("err:merge_old_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }

                {
                    let v = self.merge_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:merge_confs_map =>e{}", e))
                            .unwrap();
                    }
                }
            }
        }
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        if self.main_confs.is_none() {
            panic!(
                "err:when modules are initialized, they cannot access each other => module.name:{}",
                module.name
            )
        }
        let main_confs = self.main_confs.get::<Vec<ArcUnsafeAny>>();
        let main_conf = &main_confs[module.main_index as usize];
        let module_confs = main_conf.get::<Vec<ArcUnsafeAny>>();
        let module_conf = &module_confs[module.ctx_index as usize];
        module_conf.get_mut::<T>()
    }

    pub async fn get_main_any_conf(&self, module: ArcRwLock<Module>) -> ArcUnsafeAny {
        if !self.is_finish.load(Ordering::SeqCst) {
            let main_index = module.get().main_index;
            let self_main_index = self.main_index.load(Ordering::SeqCst);
            if main_index != self_main_index {
                log::trace!(target: "main",
                    "self_main_index:{}, main_index:{}",
                    self_main_index,
                    main_index
                );
                {
                    let v = self.init_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "init_main_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:init_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }

                {
                    let old_ms = self.old_ms.get_option().clone();
                    let v = self.merge_old_main_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_old_main_confs_map main_index:{}, name:{}", main_index, name);
                        let old_main_conf = if old_ms.is_some() {
                            Some(old_ms.as_ref().unwrap().main_confs())
                        } else {
                            None
                        };
                        (v)(old_ms, old_main_conf, self.clone(), self.main_confs())
                            .await
                            .map_err(|e| anyhow!("err:merge_old_main_confs_map =>e{}", e))
                            .unwrap();
                    }
                }

                {
                    let v = self.merge_confs_map.get_mut().remove(&main_index);
                    if v.is_some() {
                        let (name, v) = v.unwrap();
                        log::trace!(target: "main", "merge_confs_map main_index:{}, name:{}", main_index, name);
                        (v)(self.clone(), self.main_confs.clone())
                            .await
                            .map_err(|e| anyhow!("err:merge_confs_map =>e{}", e))
                            .unwrap();
                    }
                }
            }
        }
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        if self.main_confs.is_none() {
            panic!(
                "err:when modules are initialized, they cannot access each other => module.name:{}",
                module.name
            )
        }
        let main_confs = self.main_confs.get::<Vec<ArcUnsafeAny>>();
        let main_conf = main_confs[module.main_index as usize].clone();
        main_conf
    }

    pub fn get_currs_conf<T: 'static>(confs: &Vec<ArcUnsafeAny>, module: ArcRwLock<Module>) -> &T {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }

        let module_conf = &confs[module.ctx_index as usize];
        module_conf.get::<T>()
    }

    pub fn get_currs_conf_mut<T: 'static>(
        confs: &Vec<ArcUnsafeAny>,
        module: ArcRwLock<Module>,
    ) -> &mut T {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        let module_conf = &confs[module.ctx_index as usize];
        module_conf.get_mut::<T>()
    }

    pub fn get_curr_conf<T: 'static>(
        server_module_conf: &ArcUnsafeAny,
        module: ArcRwLock<Module>,
    ) -> &T {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        let server_confs = server_module_conf.get::<Vec<ArcUnsafeAny>>();
        let server_confs = &server_confs[module.ctx_index as usize];
        server_confs.get::<T>()
    }

    pub fn get_curr_conf_mut<T: 'static>(
        server_module_conf: &ArcUnsafeAny,
        module: ArcRwLock<Module>,
    ) -> &mut T {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        let server_confs = server_module_conf.get::<Vec<ArcUnsafeAny>>();
        let server_confs = &server_confs[module.ctx_index as usize];
        server_confs.get_mut::<T>()
    }

    pub fn get_currs_any_conf(
        confs: &Vec<ArcUnsafeAny>,
        module: ArcRwLock<Module>,
    ) -> ArcUnsafeAny {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }

        let module_conf = confs[module.ctx_index as usize].clone();
        module_conf
    }

    pub fn get_curr_any_conf(
        server_module_conf: &ArcUnsafeAny,
        module: ArcRwLock<Module>,
    ) -> ArcUnsafeAny {
        let module = module.get();
        if module.ctx_index_len <= 0 || module.main_index < 0 || module.ctx_index < 0 {
            panic!(
                "ctx_index_len:{},main_index:{}， ctx_index:{}",
                module.ctx_index_len, module.main_index, module.ctx_index
            )
        }
        let server_confs = server_module_conf.get::<Vec<ArcUnsafeAny>>();
        let server_confs = server_confs[module.ctx_index as usize].clone();
        server_confs
    }

    #[async_recursion(?Send)]
    pub async fn parse_config_file(
        &mut self,
        conf_arg: &mut ConfArg,
        path_full_name: &str,
        module_confs: ArcUnsafeAny,
    ) -> Result<()> {
        let file = std::fs::File::open(&path_full_name)
            .map_err(|e| anyhow!("err:path_full_name:{} => e:{}", path_full_name, e))?;
        let reader = BufReader::new(file);
        let path = Path::new(&path_full_name);

        conf_arg.file_name = path.file_name().unwrap().to_str().unwrap().to_string();
        conf_arg.path = path.parent().unwrap().to_str().unwrap().to_string();
        conf_arg.reader = ArcMutex::new(reader);
        conf_arg.line_num.store(0, Ordering::Relaxed);

        self.parse_config(conf_arg, module_confs).await?;
        return Ok(());
    }

    pub fn read_line(&self, conf_arg: &mut ConfArg) -> Result<String> {
        let mut str_raw = "".to_string();
        let n = conf_arg.reader.get_mut().read_line(&mut str_raw)?;
        if n == 0 {
            return Ok("".to_string());
        }
        conf_arg.line_num.fetch_add(1, Ordering::Relaxed);
        conf_arg.str_raw = str_raw.clone();
        return Ok(str_raw);
    }

    pub async fn parse_config(
        &mut self,
        conf_arg: &mut ConfArg,
        module_confs: ArcUnsafeAny,
    ) -> Result<()> {
        loop {
            let str_raw = self.read_line(conf_arg)?;
            if str_raw.len() <= 0 {
                if !conf_arg.is_sub_file && conf_arg.main_index.load(Ordering::SeqCst) != -1 {
                    return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                }
                break;
            }
            log::trace!(target: "main", "str_raw:{:?}", str_raw);
            log::trace!(target: "ms", "read modules session_id:{}, count:{}, str_raw:{:?} ", self.session_id, Arc::strong_count(&self._drop_test), str_raw);

            let str_left = str_raw.trim_start();
            if str_left.find(LINE_COMMENTS) == Some(0) {
                continue;
            }
            let str = str_left.trim_end();
            if str.len() == 0 {
                continue;
            }

            if str == BLOCK_COMMENTS_START {
                loop {
                    let str_raw = self.read_line(conf_arg)?;
                    if str_raw.len() <= 0 {
                        return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                    }
                    log::trace!(target: "main", "str_raw:{:?}", str_raw);
                    if str_raw.trim() == BLOCK_COMMENTS_END {
                        break;
                    }
                    continue;
                }
                continue;
            }

            let mut is_ignore = false;
            let (key, typ, value) = match str.split_once(KEY_VALUE_DELIMITER) {
                Some((k, v)) => {
                    let ks = k.trim().split_whitespace().collect::<Vec<_>>();
                    if ks.len() <= 0 || ks.len() > 2 {
                        return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                    }

                    if ks.len() == 1 {
                        (
                            ks[0].trim().to_string(),
                            CMD_TYPE_STR.to_string(),
                            v.trim_start().to_string(),
                        )
                    } else {
                        let typ = ks[1].trim();
                        match typ.split_once("_") {
                            Some((typ_key, typ_value)) => {
                                let mut _is_window = false;
                                let mut _is_linux = false;
                                #[cfg(windows)]
                                {
                                    _is_window = true;
                                }
                                #[cfg(unix)]
                                {
                                    _is_linux = true;
                                }
                                if (typ_key == "linux" || typ_key == "unix") && _is_window {
                                    is_ignore = true;
                                }

                                if (typ_key == "window" || typ_key == "windows") && _is_linux {
                                    is_ignore = true;
                                }

                                (
                                    ks[0].trim().to_string(),
                                    typ_value.to_string(),
                                    v.trim_start().to_string(),
                                )
                            }
                            None => (
                                ks[0].trim().to_string(),
                                typ.to_string(),
                                v.trim_start().to_string(),
                            ),
                        }
                    }
                }
                None => {
                    let str = match str.find(LINE_COMMENTS) {
                        Some(size) => str[0..size].to_string(),
                        None => str.to_string(),
                    };
                    let str = str.trim();
                    let len = str.len();
                    let end_char = &str[len - 1..];
                    if end_char == MAIN_START {
                        let mut str = &str[..len - 1];
                        str = str.trim();
                        (str.trim().to_string(), "".to_string(), "".to_string())
                    } else if end_char == MAIN_END {
                        if conf_arg.main_index.load(Ordering::SeqCst) == -1 {
                            return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                        }
                        conf_arg.main_index.store(-1, Ordering::SeqCst);
                        conf_arg.module_type.store(0, Ordering::SeqCst);
                        conf_arg.cmd_conf_type.store(0, Ordering::SeqCst);

                        //___wait___
                        // if conf_arg.parent_module_confs.is_empty() {
                        //     self.main_index.store(-1, Ordering::SeqCst);
                        // }
                        ("".to_string(), "".to_string(), "".to_string())
                    } else {
                        return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                    }
                }
            };

            // log::trace!(target: "main", " key:{:?}", key);
            // log::trace!(target: "main", " typ:{:?}", typ);
            // log::trace!(target: "main", " value:{:?}", value);
            if key.len() <= 0 {
                return Ok(());
            }

            let value = if typ.len() <= 0 && value.len() <= 0 {
                value
            } else if typ != CMD_TYPE_RAW {
                match value.split_once(LINE_END) {
                    Some((k, v)) => {
                        let v = v.trim();
                        if v.len() > 0 {
                            if v.find(LINE_COMMENTS) != Some(0) {
                                return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                            }
                        }
                        k.trim().to_string()
                    }
                    None => {
                        return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                    }
                }
            } else {
                if value.find(BLOCK_START) != Some(0) {
                    return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                }

                let mut raw_str = "".to_string();
                let mut value_str = Some(value[BLOCK_START.len()..].to_string());
                loop {
                    let value = if value_str.is_some() {
                        let v = value_str.unwrap();
                        value_str = None;
                        v
                    } else {
                        let str_raw = self.read_line(conf_arg)?;
                        if str_raw.len() <= 0 {
                            return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                        }
                        log::trace!(target: "main", "str_raw:{:?}", str_raw);
                        str_raw
                    };

                    match value.find(BLOCK_END) {
                        Some(index) => match value.split_once(LINE_END) {
                            Some((k, v)) => {
                                let v = v.trim();
                                if v.len() > 0 {
                                    if v.find(LINE_COMMENTS) != Some(0) {
                                        return ret_err(conf_arg, "")
                                            .map_err(|e| anyhow!("err:e:{}", e));
                                    }
                                }
                                raw_str.push_str(&k[BLOCK_START.len()..index]);
                                break;
                            }
                            None => {
                                return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                            }
                        },
                        None => {
                            raw_str.push_str(&value);
                        }
                    }
                }
                raw_str
            };

            if is_ignore {
                continue;
            }

            if key == "include" {
                if value.len() == 0 {
                    return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                }

                use std::path::PathBuf;
                let sub_path = PathBuf::from(&value);
                let new_sub_path = sub_path.strip_prefix("./");
                let new_sub_path = if new_sub_path.is_err() {
                    let new_sub_path = sub_path.strip_prefix("/");
                    if new_sub_path.is_err() {
                        Some(&value[..])
                    } else {
                        let new_sub_path = new_sub_path?;
                        new_sub_path.to_str()
                    }
                } else {
                    let new_sub_path = new_sub_path?;
                    new_sub_path.to_str()
                };

                if new_sub_path.is_none() {
                    return ret_err(conf_arg, "")
                        .map_err(|e| anyhow!("err:path:{} => e:{}", value, e));
                }
                let path_full_name = format!("{}/{}", conf_arg.path, new_sub_path.unwrap());

                let is_sub_file = conf_arg.is_sub_file;
                let file_name = conf_arg.file_name.clone();
                let path = conf_arg.path.clone();
                let line = conf_arg.line_num.load(Ordering::Relaxed);
                let reader = conf_arg.reader.clone();

                conf_arg.is_sub_file = true;
                self.parse_config_file(conf_arg, &path_full_name, module_confs.clone())
                    .await?;

                conf_arg.is_sub_file = is_sub_file;
                conf_arg.file_name = file_name;
                conf_arg.path = path;
                conf_arg.line_num.store(line, Ordering::Relaxed);
                conf_arg.reader = reader;
                continue;
            }

            conf_arg.str_key = key.clone();
            conf_arg.str_typ = typ.clone();
            conf_arg.str_value = value.clone();
            let str_key = key;
            let str_typ = typ;
            let str_value = value;
            let mut is_main_module = false;
            let mut is_find = false;
            'outer: for module in self.get_modules() {
                let (name, typ, cmds, main_index, ctx_index) = {
                    let module_ = &*module.get();
                    (
                        module_.name.clone(),
                        module_.typ,
                        module_.cmds.clone(),
                        module_.main_index,
                        module_.ctx_index,
                    )
                };
                if conf_arg.module_type.load(Ordering::SeqCst) > 0
                    && conf_arg.module_type.load(Ordering::SeqCst) & typ == 0
                {
                    continue;
                }
                for cmd in cmds.as_ref() {
                    if str_key != cmd.name {
                        continue;
                    }

                    if conf_arg.cmd_conf_type.load(Ordering::SeqCst) > 0
                        && conf_arg.cmd_conf_type.load(Ordering::SeqCst) & cmd.conf_typ == 0
                    {
                        return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                    }

                    log::trace!(target: "main", "find str_key:{:?}", str_key);
                    log::trace!(target: "main", "find str_typ:{:?}", str_typ);
                    log::trace!(target: "main", "find str_value:{:?}", str_value);

                    if cmd.typ == CMD_TYPE_MAIN {
                        log::trace!(target: "main", "========start {}", module.get().name);
                        if self.init_main_confs_map.get().get(&main_index).is_none() {
                            panic!("It has already been accessed and cannot be loaded again, module name:{}", name);
                        }
                        is_main_module = true;
                        conf_arg
                            .main_index
                            .store(main_index as i32, Ordering::SeqCst);
                        self.main_index.store(main_index as i32, Ordering::SeqCst);
                    } else if cmd.typ == CMD_TYPE_SUB {
                        is_main_module = true;
                    } else if cmd.typ == CMD_TYPE_BLOCK {
                    } else {
                        let func = VALUE_MAP.get(&str_typ);
                        if func.is_none() {
                            return ret_err(conf_arg, "").map_err(|e| anyhow!("err:e:{}", e));
                        }
                        let func = func.unwrap();
                        func(conf_arg, &str_value).map_err(|e| {
                            anyhow!(
                                "err:str_typ:{}, e:{} -> {}:{}:{}:{}",
                                str_typ,
                                e,
                                conf_arg.path,
                                conf_arg.file_name,
                                conf_arg.line_num.load(Ordering::Relaxed),
                                conf_arg.str_raw
                            )
                        })?;
                    }

                    if is_main_module || conf_arg.is_block {
                        let ret = (cmd.set)(
                            self.clone(),
                            conf_arg.clone(),
                            cmd.clone(),
                            module_confs.clone(),
                        )
                        .await;
                        ret_errs(ret, conf_arg, &str_key)?;
                    } else {
                        conf_arg.curr_module_confs = module_confs.clone();
                        self.curr_module_confs = module_confs.clone();
                        let module_conf = {
                            let module_confs = module_confs.get_mut::<Vec<ArcUnsafeAny>>();
                            module_confs[ctx_index as usize].clone()
                        };
                        let ret = (cmd.set)(
                            self.clone(),
                            conf_arg.clone(),
                            cmd.clone(),
                            module_conf.clone(),
                        )
                        .await;
                        ret_errs(ret, conf_arg, &str_key)?;
                    }
                    is_find = true;
                    break 'outer;
                }
            }
            if !is_find {
                return ret_err(conf_arg, &str_key)
                    .map_err(|e| anyhow!("err:not is_find => e:{}", e));
            }
        }
        return Ok(());
    }
}
