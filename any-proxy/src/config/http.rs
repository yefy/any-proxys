use crate::config as conf;
use crate::config::httplocal;
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
    pub sub_confs: Vec<typ::ArcUnsafeAny>,
}

impl Conf {
    pub fn new() -> Self {
        Conf {
            http_confs: Vec::new(),
            sub_confs: Vec::new(),
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![module::Cmd {
        name: "http".to_string(),
        set: |ms, conf_arg, cmd, conf| Box::pin(http(ms, conf_arg, cmd, conf)),
        typ: module::CMD_TYPE_MAIN,
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
        name: "http".to_string(),
        main_index: -1,
        ctx_index: -1,
        index: -1,
        ctx_index_len: -1,
        func: MODULE_FUNC.clone(),
        cmds: MODULE_CMDS.clone(),
        create_main_confs: Some(|ms| Box::pin(create_main_confs(ms))),
        init_main_confs: Some(|ms, main_confs| Box::pin(init_main_confs(ms, main_confs))),
        merge_old_main_confs: Some(|old_ms, old_main_conf, ms, main_conf| Box::pin(
            merge_old_main_confs(old_ms, old_main_conf, ms, main_conf)
        )),
        merge_confs: Some(|ms, main_confs| Box::pin(merge_confs(ms, main_confs))),
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
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let _http_confs = {
        let conf = conf.get::<Conf>();
        conf.http_confs.clone()
    };
    return Ok(());
}

async fn create_main_confs(ms: module::Modules) -> Result<ArcUnsafeAny> {
    let ctx_index_len = { M.get().ctx_index_len };
    if ctx_index_len <= 0 {
        panic!("ctx_index_len:{}", ctx_index_len)
    }
    let mut http_confs: Vec<typ::ArcUnsafeAny> = Vec::with_capacity(ctx_index_len as usize);

    for module in module::get_modules().get().get_modules() {
        let (typ, func) = {
            let module_ = &*module.get();
            (module_.typ, module_.func.clone())
        };
        if typ & conf::MODULE_TYPE_HTTP == 0 {
            continue;
        }
        let conf = (func.create_conf)(ms.clone()).await?;
        http_confs.push(conf);
    }
    Ok(typ::ArcUnsafeAny::new(Box::new(http_confs)))
}

async fn merge_confs(ms: module::Modules, conf: typ::ArcUnsafeAny) -> Result<()> {
    let http_confs = {
        let main_index = M.get().main_index;
        if main_index < 0 {
            panic!("main_index:{}", main_index)
        }
        let main_confs = conf.get_mut::<Vec<typ::ArcUnsafeAny>>();
        let http_main_conf = main_confs[main_index as usize].clone();
        let http_confs = http_main_conf.get_mut::<Vec<typ::ArcUnsafeAny>>().clone();
        http_confs
    };

    for module in ms.get_modules() {
        if module.get().typ & conf::MODULE_TYPE_HTTP == 0 {
            continue;
        }
        let (merge_conf, http_conf) = {
            let module_ = module.get();
            let merge_conf = module_.func.merge_conf.clone();
            let http_conf = http_confs[module_.ctx_index as usize].clone();
            (merge_conf, http_conf)
        };

        (merge_conf)(ms.clone(), None, http_conf).await?;
    }

    let sub_confs = {
        let ctx_index = { M.get().ctx_index };
        if ctx_index < 0 {
            panic!("ctx_index:{}", ctx_index)
        }
        let http_conf = http_confs[ctx_index as usize].clone();
        let http_conf = http_conf.get_mut::<Conf>();
        http_conf.sub_confs.clone()
    };

    for server_conf in sub_confs {
        let (http_confs, server_confs, sub_confs) = {
            let server_conf = server_conf.get_mut::<httpserver::Conf>();
            (
                server_conf.http_confs.clone(),
                server_conf.server_confs.clone(),
                server_conf.sub_confs.clone(),
            )
        };
        for module in ms.get_modules() {
            if module.get().typ & conf::MODULE_TYPE_HTTP == 0 {
                continue;
            }
            let (merge_conf, http_conf, server_conf) = {
                let module_ = module.get();
                let merge_conf = module_.func.merge_conf.clone();
                let http_conf = http_confs[module_.ctx_index as usize].clone();
                let server_conf = server_confs[module_.ctx_index as usize].clone();
                (merge_conf, http_conf, server_conf)
            };

            (merge_conf)(ms.clone(), Some(http_conf), server_conf).await?;
        }
        for local_conf in sub_confs {
            let (server_confs, local_confs) = {
                let local_conf = local_conf.get_mut::<httplocal::Conf>();
                (
                    local_conf.server_confs.clone(),
                    local_conf.local_confs.clone(),
                )
            };
            for module in ms.get_modules() {
                if module.get().typ & conf::MODULE_TYPE_HTTP == 0 {
                    continue;
                }
                let (merge_conf, server_conf, local_conf) = {
                    let module_ = module.get();
                    let merge_conf = module_.func.merge_conf.clone();
                    let server_conf = server_confs[module_.ctx_index as usize].clone();
                    let local_conf = local_confs[module_.ctx_index as usize].clone();
                    (merge_conf, server_conf, local_conf)
                };
                (merge_conf)(ms.clone(), Some(server_conf), local_conf).await?;
            }
        }
    }
    return Ok(());
}

async fn merge_old_main_confs(
    old_ms: Option<module::Modules>,
    old_main_conf: Option<ArcUnsafeAny>,
    ms: module::Modules,
    main_conf: ArcUnsafeAny,
) -> Result<()> {
    let main_index = M.get().main_index;
    if main_index < 0 {
        panic!("main_index:{}", main_index)
    }
    let old_http_confs = if old_main_conf.is_some() {
        let old_main_confs = old_main_conf
            .as_ref()
            .unwrap()
            .get_mut::<Vec<typ::ArcUnsafeAny>>();
        let old_http_main_conf = old_main_confs[main_index as usize].clone();
        let old_http_confs = old_http_main_conf
            .get_mut::<Vec<typ::ArcUnsafeAny>>()
            .clone();
        Some(old_http_confs)
    } else {
        None
    };
    let main_confs = main_conf.get_mut::<Vec<typ::ArcUnsafeAny>>();
    let http_main_conf = main_confs[main_index as usize].clone();
    let http_confs = http_main_conf.get_mut::<Vec<typ::ArcUnsafeAny>>().clone();

    for module in ms.get_modules() {
        let (typ, ctx_index, func, name, main_index) = {
            let module_ = &*module.get();
            (
                module_.typ,
                module_.ctx_index,
                module_.func.clone(),
                module_.name.clone(),
                module_.main_index,
            )
        };
        if typ & conf::MODULE_TYPE_HTTP == 0 {
            continue;
        }
        let old_http_conf = if old_http_confs.is_some() {
            let old_http_confs = old_http_confs.as_ref().unwrap();
            Some(old_http_confs[ctx_index as usize].clone())
        } else {
            None
        };
        log::trace!(
            "http merge_old_main_confs name:{}, typ:{}, main_index:{}, ctx_index:{}",
            name,
            typ,
            main_index,
            ctx_index
        );
        (func.merge_old_conf)(
            old_ms.clone(),
            old_main_conf.clone(),
            old_http_conf,
            ms.clone(),
            main_conf.clone(),
            http_confs[ctx_index as usize].clone(),
        )
        .await?;
    }
    Ok(())
}

async fn init_main_confs(ms: module::Modules, conf: typ::ArcUnsafeAny) -> Result<()> {
    let main_index = M.get().main_index;
    if main_index < 0 {
        panic!("main_index:{}", main_index)
    }
    let main_confs = conf.get_mut::<Vec<typ::ArcUnsafeAny>>();
    let http_main_conf = main_confs[main_index as usize].clone();
    let http_confs = http_main_conf.get_mut::<Vec<typ::ArcUnsafeAny>>().clone();

    for module in ms.get_modules() {
        let (typ, ctx_index, func, name, main_index) = {
            let module_ = &*module.get();
            (
                module_.typ,
                module_.ctx_index,
                module_.func.clone(),
                module_.name.clone(),
                module_.main_index,
            )
        };
        if typ & conf::MODULE_TYPE_HTTP == 0 {
            continue;
        }
        log::trace!(
            "http init_main_confs name:{}, typ:{}, main_index:{}, ctx_index:{}",
            name,
            typ,
            main_index,
            ctx_index
        );
        (func.init_conf)(
            ms.clone(),
            conf.clone(),
            http_confs[ctx_index as usize].clone(),
        )
        .await?;
    }
    Ok(())
}

async fn http(
    ms: module::Modules,
    mut conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    conf_arg
        .module_type
        .store(conf::MODULE_TYPE_HTTP, Ordering::SeqCst);
    conf_arg
        .cmd_conf_type
        .store(conf::CMD_CONF_TYPE_MAIN, Ordering::SeqCst);

    let main_index = M.get().main_index;
    if main_index < 0 {
        panic!("main_index:{}", main_index)
    }
    let main_confs = conf.get_mut::<Vec<typ::ArcUnsafeAny>>();
    let http_main_conf = main_confs[main_index as usize].clone();
    let http_confs = http_main_conf.get_mut::<Vec<typ::ArcUnsafeAny>>().clone();

    {
        let ctx_index = { M.get().ctx_index };
        if ctx_index < 0 {
            panic!("ctx_index:{}", ctx_index)
        }
        let http_conf = http_confs[ctx_index as usize].clone();

        let http_conf = http_conf.get_mut::<Conf>();
        http_conf.http_confs = http_confs.clone();
    }

    ms.parse_config(&mut conf_arg, http_main_conf).await?;

    return Ok(());
}
