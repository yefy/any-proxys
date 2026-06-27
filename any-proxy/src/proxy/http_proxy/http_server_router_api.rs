use crate::proxy::http_proxy::http_router::Request;
use crate::proxy::http_proxy::http_router::{router_handle_anyproxy_test, Params};
use crate::proxy::http_proxy::http_router::{AnyHttpRouter, Json};
use any_base::typ::ArcRwLock;
use hyper::Method;
use lazy_static::lazy_static;
use log4rs::Log4Handle;
use std::collections::HashMap;

lazy_static! {
    pub static ref LOG4_HANDLE: ArcRwLock<Log4Handle> = ArcRwLock::default();
}

pub fn get_Log4_Handle() -> Option<Log4Handle> {
    let log4_Handle = LOG4_HANDLE.get();
    if log4_Handle.is_none() {
        return None;
    }
    return Some((*log4_Handle).clone());
}

pub fn set_Log4_Handle(log4_Handle: Log4Handle) {
    LOG4_HANDLE.set(log4_Handle);
}

pub async fn register_router_handles() -> anyhow::Result<()> {
    use crate::config::net_server_http_router;

    net_server_http_router::add_router_handle("", router_handle_anyproxy).await?;
    net_server_http_router::add_router_handle("anyproxy", router_handle_anyproxy).await?;
    net_server_http_router::add_router_handle("anyproxy_test", router_handle_anyproxy_test).await?;

    Ok(())
}

pub async fn router_handle_anyproxy(_name: String) -> anyhow::Result<Option<AnyHttpRouter>> {
    let mut router = AnyHttpRouter::with_any_state(());
    router.add_json(Method::POST, "/proxy/hot_io_percent", proxy_hot_io_percent);
    router.add_json(Method::GET, "/log/reopen", log_reopen);
    return Ok(Some(router));
}

//curl -X POST -H "Content-Type: application/json" -d '{"proxy_cache_1":90, "proxy_cache_2":0}' http://www.example.cn:11210/proxy/hot_io_percent -k -v
//curl http://www.example.cn:11210/log/reopen -k -v
pub async fn proxy_hot_io_percent(
    req: Request,
    params: Params,
    Json(req_data): Json<HashMap<String, u64>>,
    _state: (),
) -> anyhow::Result<()> {
    log::info!(
        "http_router hot_io_percent req:{:?}, params:{:?}, req_data:{:?}",
        req,
        params,
        req_data,
    );

    let scc = req.stream_request()?.http_arg.stream_info.get().scc.clone();

    use crate::config::net_core_proxy;
    let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;
    *net_core_proxy_main_conf.main.hot_io_percent_map.get_mut() = req_data;

    Ok(())
}

pub async fn log_reopen(
    req: Request,
    params: Params,
    _json: Json<()>,
    _state: (),
) -> anyhow::Result<serde_json::Value> {
    log::info!(
        "http_router hot_io_percent req:{:?}, params:{:?}",
        req,
        params,
    );

    let log4_handle = get_Log4_Handle();
    if log4_handle.is_none() {
        return Err(anyhow::anyhow!("log4_handle is nil"));
    }
    let log4_handle = log4_handle.unwrap();

    let reopen_wait = log4_handle.reopen();
    let _ = reopen_wait.recv().await;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000 * 2)).await;
    Ok(serde_json::json!({
        "msg": "ok",
    }))
}
