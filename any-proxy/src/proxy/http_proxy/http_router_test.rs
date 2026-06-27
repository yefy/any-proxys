use super::http_router::{
    AnyHttpRouter, FromHttpRouterState, HttpRouterStateBox, Json, Params, Request,
};
use bytes::{Buf, Bytes};
use futures_util::StreamExt;
use hyper::{Body, Method, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
struct State {
    num: i32,
}

#[derive(Clone, Debug, PartialEq)]
struct OtherState {
    value: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct BodyData {
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutputData {
    id: String,
    param: String,
    num: i32,
}

async fn response_json(response: Response<Body>) -> Value {
    let body = response_body_bytes(response).await;
    serde_json::from_slice(body.as_slice()).unwrap()
}

async fn response_text(response: Response<Body>) -> String {
    let body = response_body_bytes(response).await;
    String::from_utf8(body).unwrap()
}

async fn response_body_bytes(response: Response<Body>) -> Vec<u8> {
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        let mut chunk = chunk.unwrap();
        while chunk.has_remaining() {
            let data = chunk.chunk();
            bytes.extend_from_slice(data);
            let len = data.len();
            chunk.advance(len);
        }
    }
    bytes
}

#[tokio::test]
async fn add_json_uses_default_body_when_request_body_is_empty() {
    async fn handler(
        _req: Request,
        _params: Params,
        Json(body): Json<BodyData>,
        state: State,
    ) -> anyhow::Result<OutputData> {
        Ok(OutputData {
            id: body.id,
            param: "".to_string(),
            num: state.num,
        })
    }

    let mut router = AnyHttpRouter::with_any_state(State { num: 7 });
    router.add_json(Method::GET, "/empty", handler);

    let response = router
        .call(Request::new_test(Method::GET, "/empty", None))
        .await;
    let json = response_json(response).await;

    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["id"], "");
    assert_eq!(json["result"]["num"], 7);
}

#[tokio::test]
async fn add_json_parses_body_params_and_typed_state() {
    async fn handler(
        _req: Request,
        params: Params,
        Json(body): Json<BodyData>,
        state: State,
    ) -> anyhow::Result<OutputData> {
        Ok(OutputData {
            id: body.id,
            param: params.get("id").cloned().unwrap_or_default(),
            num: state.num,
        })
    }

    let mut router = AnyHttpRouter::with_any_state(State { num: 11 });
    router.add_json(Method::POST, "/user/{id}", handler);

    let response = router
        .call(Request::new_test(
            Method::POST,
            "/user/abc",
            Some(Bytes::from_static(br#"{"id":"body-id"}"#)),
        ))
        .await;
    let json = response_json(response).await;

    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["id"], "body-id");
    assert_eq!(json["result"]["param"], "abc");
    assert_eq!(json["result"]["num"], 11);
}

#[tokio::test]
async fn add_extracts_arc_state_for_hyper_response() {
    async fn handler(
        _req: Request,
        params: Params,
        state: Arc<State>,
    ) -> anyhow::Result<Response<Body>> {
        let id = params.get("id").cloned().unwrap_or_default();
        Ok(Response::builder()
            .status(StatusCode::CREATED)
            .body(Body::from(format!("{}:{}", id, state.num)))?)
    }

    let mut router = AnyHttpRouter::with_any_state(Arc::new(State { num: 77 }));
    router.add(Method::POST, "/hyper/{id}", handler);

    let response = router
        .call(Request::new_test(Method::POST, "/hyper/xyz", None))
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response_text(response).await, "xyz:77");
}

#[tokio::test]
async fn state_type_mismatch_returns_error_response() {
    async fn handler(
        _req: Request,
        _params: Params,
        _json: Json<()>,
        _state: OtherState,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    let mut router = AnyHttpRouter::with_any_state(State { num: 1 });
    router.add_json(Method::GET, "/wrong-state", handler);

    let response = router
        .call(Request::new_test(Method::GET, "/wrong-state", None))
        .await;
    let json = response_json(response).await;

    assert_eq!(json["success"], false);
    assert!(json["msg"]
        .as_str()
        .unwrap()
        .contains(std::any::type_name::<OtherState>()));
}

#[tokio::test]
async fn invalid_json_returns_error_response() {
    async fn handler(
        _req: Request,
        _params: Params,
        _json: Json<BodyData>,
        _state: State,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    let mut router = AnyHttpRouter::with_any_state(State { num: 1 });
    router.add_json(Method::POST, "/bad-json", handler);

    let response = router
        .call(Request::new_test(
            Method::POST,
            "/bad-json",
            Some(Bytes::from_static(b"not json")),
        ))
        .await;
    let json = response_json(response).await;

    assert_eq!(json["success"], false);
    assert!(json["msg"]
        .as_str()
        .unwrap()
        .contains("serde_json::from_slice err"));
}

#[tokio::test]
async fn missing_route_returns_not_found_response() {
    let router = AnyHttpRouter::with_any_state(State { num: 1 });

    let response = router
        .call(Request::new_test(Method::GET, "/missing", None))
        .await;
    let json = response_json(response).await;

    assert_eq!(json["success"], false);
    assert!(json["msg"].as_str().unwrap().contains("404 not found"));
}

#[test]
fn from_http_router_state_extracts_plain_and_arc_state() {
    let plain: State =
        FromHttpRouterState::from_http_router_state(HttpRouterStateBox::new(State { num: 3 }))
            .expect("plain state should downcast");
    assert_eq!(plain.num, 3);

    let arc_state = Arc::new(State { num: 4 });
    let extracted: Arc<State> =
        FromHttpRouterState::from_http_router_state(HttpRouterStateBox::new(arc_state))
            .expect("arc state should downcast");
    assert_eq!(extracted.num, 4);
}
