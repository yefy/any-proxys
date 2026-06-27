extern crate anyhow;
extern crate http;

use any_proxy;
use any_proxy::AnyHttpRouter;
use any_proxy::AsyncFunc;
use any_proxy::ExecutorsLocal;
use any_proxy::HttpRouterBody;
use any_proxy::HttpRouterJson;
use any_proxy::HttpRouterParams;
use any_proxy::HttpRouterRequest;
use any_proxy::HttpRouterResponse;
use anyhow::Context;
use bytes::Bytes;
use http::{HeaderValue, Method};
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

//如何运行
//C:\Users\yefy\Desktop\yefy\develop\git-project\any-proxys\any-example\anyproxy_router
//+1.96.0 run --profile bench-dev -- -c conf/anyproxy_simple_http_router.conf

fn main() {
    println!("start");
    if let Err(e) = do_main() {
        println!("err:{:?}", e);
    }
    println!("end");
}

fn do_main() -> anyhow::Result<()> {
    any_proxy::run(
        0,
        Some(AsyncFunc {
            init: |executors| Box::pin(init(executors)),
            run: |executors, state| Box::pin(run(executors, state)),
            sig_stop: |executors, state| Box::pin(sig_stop(executors, state)),
            sig_quit: |executors, state| Box::pin(sig_quit(executors, state)),
            sig_reload: |executors, state| Box::pin(sig_reload(executors, state)),
            sig_check: |executors, state| Box::pin(sig_check(executors, state)),
            sig_reinit: |executors, state| Box::pin(sig_reinit(executors, state)),
        }),
    )?;
    Ok(())
}

async fn init(_executors: ExecutorsLocal) -> anyhow::Result<Arc<AnyProxyRouterTestData>> {
    let state = Arc::new(AnyProxyRouterTestData::new(77));
    log::info!("register num:{}", state.num);
    let router_state = state.clone();
    any_proxy::add_router_handle("anyproxy_router_test", move |name| {
        let state = router_state.clone();
        Box::pin(router_handle(name, state))
    })
    .await?;
    Ok(state)
}

async fn run(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("run, num:{}", state.num);
    Ok(())
}

async fn sig_stop(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("sig_stop, num:{}", state.num);
    Ok(())
}

async fn sig_quit(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("sig_quit, num:{}", state.num);
    Ok(())
}

async fn sig_reload(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("sig_reload, num:{}", state.num);
    Ok(())
}

async fn sig_check(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("sig_check, num:{}", state.num);
    Ok(())
}

async fn sig_reinit(
    _executors: ExecutorsLocal,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!("sig_reinit, num:{}", state.num);
    Ok(())
}

//curl http://www.example.cn:11210/test2/hello -k -v
//curl http://www.example.cn:11210/test2/echo -k -v
//curl http://www.example.cn:11210/test2/ok -k -v
//curl -X POST -H "Content-Type: application/json" -d '{"id":"123"}' http://www.example.cn:11210/test2/user/123 -k -v
//curl -X POST -H "Content-Type: application/json" -d '{"id":"123"}' http://www.example.cn:11210/test2/ok_hyper -k -v
pub async fn router_handle(
    _name: String,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<Option<AnyHttpRouter>> {
    let mut router = AnyHttpRouter::with_any_state(state);
    router.add_json(Method::GET, "/test2/hello", hello_test_2);
    router.add_json(Method::GET, "/test2/echo", echo_test_2);
    router.add_json(Method::POST, "/test2/user/{id}", user_test_2);
    router.add_json(Method::GET, "/test2/ok", ok_test_2);
    router.add(Method::POST, "/test2/ok_hyper", ok_hyper_test_2);
    return Ok(Some(router));
}

#[derive(Clone)]
pub struct AnyProxyRouterTestData {
    num: i32,
}

impl AnyProxyRouterTestData {
    pub fn new(num: i32) -> Self {
        AnyProxyRouterTestData { num }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoTestData {
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTestData {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserTestDataReq {
    pub id: String,
}

pub async fn echo_test_2(
    req: HttpRouterRequest,
    params: HttpRouterParams,
    _json: HttpRouterJson<()>,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<EchoTestData> {
    log::info!(
        "echo_test_2 req:{:?}, params:{:?}, num:{}",
        req,
        params,
        state.num
    );
    Ok(EchoTestData {
        data: "echo_test_2".to_string(),
    })
}

pub async fn user_test_2(
    req: HttpRouterRequest,
    params: HttpRouterParams,
    HttpRouterJson(req_data): HttpRouterJson<UserTestDataReq>,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<UserTestData> {
    log::info!(
        "user_test_2 req:{:?}, params:{:?}, num:{}",
        req,
        params,
        state.num
    );
    log::info!("req_data:{:?}", req_data);

    let id = params.get("id").cloned().unwrap_or_default();

    if req_data.id != id {
        return Err(anyhow::anyhow!("req_data.id:{} != id:{}", req_data.id, id));
    }

    Ok(UserTestData {
        id,
        name: "张三_2".to_string(),
    })
}

pub async fn hello_test_2(
    req: HttpRouterRequest,
    params: HttpRouterParams,
    _json: HttpRouterJson<()>,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<String> {
    log::info!(
        "hello_test_2 req:{:?}, params:{:?}, num:{}",
        req,
        params,
        state.num
    );
    Ok("hello_test_2".to_string())
}

pub async fn ok_test_2(
    req: HttpRouterRequest,
    params: HttpRouterParams,
    _json: HttpRouterJson<()>,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<()> {
    log::info!(
        "ok_test_2 req:{:?}, params:{:?}, num:{}",
        req,
        params,
        state.num
    );
    Ok(())
}

pub async fn ok_hyper_test_2(
    req: HttpRouterRequest,
    params: HttpRouterParams,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<HttpRouterResponse<HttpRouterBody>> {
    let body = req.read_body().await.here()?;
    let body = body.unwrap_or(Bytes::new());
    let body = String::from_utf8(body.to_vec()).here()?;
    log::info!(
        "ok_hyper_test_2 req:{:?}, params:{:?}, num:{}, body:{}",
        req,
        params,
        state.num,
        body,
    );
    let resp_body = "ok_hyper_test_2".to_string();
    let resp_body_len = resp_body.len();
    let mut response = HttpRouterResponse::new(HttpRouterBody::from(resp_body));
    response.headers_mut().insert(
        http::header::CONTENT_LENGTH,
        HeaderValue::from(resp_body_len),
    );
    Ok(response)
}
