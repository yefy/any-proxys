use axum::extract::ConnectInfo;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};
use log4rs::Log4Handle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use rivetx_core::task_group::TaskGroup;
use anyhow::anyhow;
use anyhow::Result;

pub fn init_file(path: &str) -> anyhow::Result<Log4Handle> {
    let log4_handle = log4rs::init_file(path, Default::default())
        .map_err(|e| anyhow::anyhow!("err:log4rs::init_file => e:{:?}", e))?;
    Ok(log4_handle)
}

pub fn http_server(port: u16, log4_handle: Log4Handle, mut executor: ExecutorLocalSpawn) -> anyhow::Result<TaskGroup> {
    let task_group = TaskGroup::new();
    let mut shutdown_thread_rx = task_group.subscribe();
    let wgg = task_group.guard_add();
    executor._start(
        #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
        move |_| async move {
            let _wgg = wgg;
            let ret: Result<()> = async {
                tokio::select! {
                        biased;
                        _ = shutdown_thread_rx.recv() => {
                            return Ok(());
                        }
                        ret = do_http_server(port, log4_handle) => {
                            return ret;
                        }
                        else => {
                            return Err(anyhow!("err:start_cmd"))?;
                        }
                    }
            }
                .await;
            if let Err(e) = ret {
                log::error!("err:do_http_server => err:{}", e);
            }
            Ok(())
        },
    );

    Ok(task_group)
}
pub async fn do_http_server(port: u16, log4_handle: Log4Handle) -> anyhow::Result<()> {
    let routes = axum::Router::new()
        .route("/log/reopen", axum::routing::get(log_reopen))
        .route("/test/get", axum::routing::get(test_get))
        .route("/test/echo", axum::routing::post(test_echo))
        .route("/test/echo2", axum::routing::post(test_echo2))
        .route("/test/echo3", axum::routing::post(test_echo3))
        .route("/test/echo4", axum::routing::post(test_echo4))
        .route("/", axum::routing::get(|| async { "Hello Axum" }))
        .with_state(log4_handle)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024));

    let addr = format!("0.0.0.0:{}", port);
    log::info!("main bind addr:{}", addr);
    let listener = tokio::net::TcpListener::bind(addr.clone())
        .await
        .map_err(|e| anyhow::anyhow!("err:bind => addr:{}, e:{}", addr, e))?;

    axum::serve(
        listener,
        routes.into_make_service_with_connect_info::<SocketAddr>(),
    )
        .await
        .map_err(|e| anyhow::anyhow!("err:axum::serve => e:{}", e))?;
    Ok(())
}

//curl http://127.0.0.1:58080/log/reopen |jq
pub async fn log_reopen(
    State(log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let reopen_wait = log4_handle.reopen();
    let _ = reopen_wait.recv().await;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000 * 2)).await;
    axum::Json(json!({
        "success": true,
        "msg": "log_reopen",
        "client_addr": addr,
    }))
}

//curl http://127.0.0.1:58080/test/get |jq
pub async fn test_get(
    State(_log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    axum::Json(json!({
        "success": true,
        "msg": "test_get",
        "client_addr": addr,
    }))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Echo {
    data: String,
}

//curl -X POST http://127.0.0.1:58080/test/echo -H "Content-Type: application/json" -d '{"data": "echo"}' |jq
pub async fn test_echo(
    State(_log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(data): Json<Echo>,
) -> impl IntoResponse {
    axum::Json(json!({
        "success": true,
        "msg": "test_echo",
        "client_addr": addr,
        "result": data,
    }))
}

//curl -X POST http://127.0.0.1:58080/test/echo2 -H "Content-Type: application/json" -d '{"data": "echo2"}' |jq
pub async fn test_echo2(
    State(_log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(data): Json<Value>,
) -> impl IntoResponse {
    axum::Json(json!({
        "success": true,
        "msg": "test_echo2",
        "client_addr": addr,
        "result": data,
    }))
}

//curl -X POST http://127.0.0.1:58080/test/echo3 -H "Content-Type: application/json" -d '{"data": "echo3"}' |jq
pub async fn test_echo3(
    State(_log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(data): Json<Value>,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "msg": "test_echo3",
            "client_addr": addr,
            "result": data,
        })),
    )
}

//curl -X POST http://127.0.0.1:58080/test/echo4 -H "Content-Type: application/json" -d '{"data": "echo4"}' |jq
pub async fn test_echo4(
    State(_log4_handle): State<Log4Handle>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(data): Json<Value>,
) -> Json<Value> {
    Json(json!({
        "success": true,
        "msg": "test_echo4",
        "client_addr": addr,
        "result": data,
    }))
}
