use crate::proxy::http_proxy::http_router::{hello_test, ok_test};
use crate::proxy::http_proxy::http_router::{echo_test, user_test, HttpRouter};
use hyper::Method;
use crate::proxy::http_proxy::http_router::Request;
use crate::proxy::http_proxy::http_router::Params;
use std::collections::HashMap;

pub async fn router_handle(router: &mut HttpRouter) -> anyhow::Result<()> {
    router.add(Method::GET, "/test/hello", hello_test);
    router.add(Method::GET, "/test/echo", echo_test);
    router.add(Method::POST, "/test/user/{id}", user_test);
    router.add(Method::GET, "/test/ok", ok_test);
    router.add(Method::POST, "/proxy/hot_io_percent", proxy_hot_io_percent);
    return Ok(());
}


pub async fn proxy_hot_io_percent(req: Request, params: Params) -> anyhow::Result<()> {
    log::info!("http_router hot_io_percent req:{:?}, params:{:?}", req, params);
    if req.body.is_none() {
        return Err(anyhow::anyhow!("body is nil:{:?}", req.body));
    }
    let req_data: HashMap<String, u64> = serde_json::from_slice(req.body.as_ref().unwrap())
        .map_err(|e| anyhow::anyhow!("serde_json::from_str err:{:?}", e))?;
    log::info!("http_router hot_io_percent req_data:{:?}", req_data);

    let scc = req.r.http_arg.stream_info.get().scc.clone();

    use crate::config::net_core_proxy;
    let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;
    *net_core_proxy_main_conf.main.hot_io_percent_map.get_mut() = req_data;

    Ok(())
}
