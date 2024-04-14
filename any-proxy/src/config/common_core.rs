use crate::config as conf;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

lazy_static! {
    pub static ref SESSION_ID: Arc<AtomicU64> = Arc::new(AtomicU64::new(100000));
}
lazy_static! {
    pub static ref TMPFILE_ID: Arc<AtomicU64> = Arc::new(AtomicU64::new(123456));
}

pub struct Conf {
    pub cpu_affinity: bool,
    pub reuseport: bool,
    pub worker_threads: usize,
    pub max_connections: i32,
    pub shutdown_timeout: u64,
    pub max_open_file_limit: u64,
    pub worker_threads_blocking: usize,
    pub memlock_rlimit_curr: u64,
    pub memlock_rlimit_max: u64,
    pub session_id: Arc<AtomicU64>,
    pub tmpfile_id: Arc<AtomicU64>,
}

impl Conf {
    pub fn new() -> Self {
        let mut conf = Conf {
            cpu_affinity: false,
            reuseport: false,
            worker_threads: 0,
            max_connections: 0,
            shutdown_timeout: 30,
            max_open_file_limit: 102400,
            worker_threads_blocking: 512,
            memlock_rlimit_curr: 128 << 20,
            memlock_rlimit_max: 128 << 20,
            session_id: SESSION_ID.clone(),
            tmpfile_id: TMPFILE_ID.clone(),
        };
        let core_ids = core_affinity::get_core_ids().unwrap();
        conf.worker_threads = core_ids.len();
        if conf.worker_threads == 0 {
            conf.worker_threads = 1;
        }
        conf
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "cpu_affinity".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(cpu_affinity(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "reuseport".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(reuseport(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "worker_threads".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(worker_threads(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "max_connections".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(max_connections(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "shutdown_timeout".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(shutdown_timeout(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "max_open_file_limit".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(max_open_file_limit(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "worker_threads_blocking".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(worker_threads_blocking(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "memlock_rlimit_curr".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(memlock_rlimit_curr(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "memlock_rlimit_max".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(memlock_rlimit_max(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
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
        name: "common_core".to_string(),
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
        typ: conf::MODULE_TYPE_COMMON,
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

async fn cpu_affinity(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.cpu_affinity = *conf_arg.value.get::<bool>();
    log::trace!(target: "main", "common_core.rs c.cpu_affinity:{:?}", c.cpu_affinity);
    return Ok(());
}

async fn reuseport(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.reuseport = *conf_arg.value.get::<bool>();
    #[cfg(not(unix))]
    {
        if c.reuseport {
            return Err(anyhow::anyhow!("windows not support reuseport"));
        }
    };

    log::trace!(target: "main", "common_core.rs c.reuseport:{:?}", c.reuseport);
    return Ok(());
}

async fn worker_threads(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.worker_threads = *conf_arg.value.get::<usize>();
    if c.worker_threads == 0 {
        let core_ids = core_affinity::get_core_ids().unwrap();
        c.worker_threads = core_ids.len();
    }
    if c.worker_threads == 0 {
        c.worker_threads = 1;
    }
    log::trace!(target: "main", "common_core.rs c.worker_threads:{:?}", c.worker_threads);
    return Ok(());
}

async fn max_connections(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.max_connections = *conf_arg.value.get::<i32>();
    log::trace!(target: "main", "common_core.rs c.max_connections:{:?}", c.max_connections);
    return Ok(());
}

async fn shutdown_timeout(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.shutdown_timeout = *conf_arg.value.get::<u64>();
    log::trace!(target: "main", "common_core.rs c.shutdown_timeout:{:?}", c.shutdown_timeout);
    return Ok(());
}

async fn max_open_file_limit(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.max_open_file_limit = *conf_arg.value.get::<u64>();
    log::trace!(target: "main",
        "common_core.rs c.debug_is_print_config:{:?}",
        c.max_open_file_limit
    );
    return Ok(());
}
async fn worker_threads_blocking(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.worker_threads_blocking = *conf_arg.value.get::<usize>();
    log::trace!(target: "main",
        "common_core.rs c.worker_threads_blocking:{:?}",
        c.worker_threads_blocking
    );
    return Ok(());
}
async fn memlock_rlimit_curr(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.memlock_rlimit_curr = *conf_arg.value.get::<u64>();
    log::trace!(target: "main",
        "common_core.rs c.memlock_rlimit_curr:{:?}",
        c.memlock_rlimit_curr
    );
    return Ok(());
}
async fn memlock_rlimit_max(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.memlock_rlimit_max = *conf_arg.value.get::<u64>();
    log::trace!(target: "main",
        "common_core.rs c.memlock_rlimit_max:{:?}",
        c.memlock_rlimit_max
    );
    return Ok(());
}
