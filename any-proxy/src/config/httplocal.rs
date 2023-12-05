use crate::config as conf;
use crate::config::httpserver;
use any_base::module::module;
use any_base::typ;
use any_base::typ::ArcUnsafeAny;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct Conf {
    pub http_confs: Vec<typ::ArcUnsafeAny>,
    pub server_confs: Vec<typ::ArcUnsafeAny>,
    pub local_confs: Vec<typ::ArcUnsafeAny>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            http_confs: Vec::new(),
            server_confs: Vec::new(),
            local_confs: Vec::new(),
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "local".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(local(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_SUB,
        conf_typ: conf::CMD_CONF_TYPE_MAIN | conf::CMD_CONF_TYPE_SERVER | conf::CMD_CONF_TYPE_LOCAL,
    }]);
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
        name: "httplocal".to_string(),
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
        typ: conf::MODULE_TYPE_HTTP,
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
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}

async fn local(
    ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let server_confs = {
        let server_confs = conf.get_mut::<Vec<typ::ArcUnsafeAny>>();
        server_confs.clone()
    };
    let ctx_index_len = { M.get().ctx_index_len };

    if ctx_index_len <= 0 {
        panic!("ctx_index_len:{}", ctx_index_len)
    }
    let mut local_confs: Vec<typ::ArcUnsafeAny> = Vec::with_capacity(ctx_index_len as usize);

    for module in ms.get_modules() {
        let (typ, func) = {
            let module_ = &*module.get();
            (module_.typ, module_.func.clone())
        };
        if typ & conf::MODULE_TYPE_HTTP == 0 {
            continue;
        }
        let conf = (func.create_conf)(ms.clone()).await?;
        local_confs.push(conf);
    }

    {
        let ctx_index = httpserver::module().get().ctx_index;
        let server_conf = server_confs[ctx_index as usize].clone();
        let server_conf_ = server_conf.get_mut::<httpserver::Conf>();

        let ctx_index = module().get().ctx_index;
        let local_conf = local_confs[ctx_index as usize].clone();
        let local_conf_ = local_conf.get_mut::<Conf>();
        local_conf_.http_confs = server_conf_.http_confs.clone();
        local_conf_.server_confs = server_confs.clone();
        local_conf_.local_confs = local_confs.clone();

        server_conf_.sub_confs.push(local_conf.clone());
    }

    let local_confs_ = typ::ArcUnsafeAny::new(Box::new(local_confs.clone()));
    let mut conf_arg_sub = module::ConfArg::new();
    conf_arg_sub
        .module_type
        .store(conf::MODULE_TYPE_HTTP, Ordering::SeqCst);
    conf_arg_sub
        .cmd_conf_type
        .store(conf::CMD_CONF_TYPE_LOCAL, Ordering::SeqCst);
    conf_arg_sub
        .main_index
        .store(conf_arg.main_index.load(Ordering::SeqCst), Ordering::SeqCst);
    conf_arg_sub.reader = conf_arg.reader.clone();
    conf_arg_sub.file_name = conf_arg.file_name.clone();
    conf_arg_sub.path = conf_arg.path.clone();
    conf_arg_sub.line_num = conf_arg.line_num.clone();
    conf_arg_sub.parent_module_confs = server_confs;

    ms.parse_config(&mut conf_arg_sub, local_confs_).await?;
    return Ok(());
}
