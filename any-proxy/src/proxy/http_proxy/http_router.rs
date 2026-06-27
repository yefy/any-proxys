use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use anyhow::Context;
use bytes::Bytes;
use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::http::header::HeaderValue;
use hyper::Method;
use hyper::{Body, Response};
use log;
use matchit::Router;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

pub trait IntoResponse {
    fn into_response(self) -> Response<Body>;
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpRouterResult<T> {
    pub success: bool,
    pub code: i32,
    pub msg: String,
    pub result: Option<T>,
}

impl<T> IntoResponse for HttpRouterResult<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response<Body> {
        json_response(serde_json::to_string(&self).unwrap_or_else(|_| {
            r#"{"success":false,"code":-1,"msg":"json serialize error","result":null}"#.to_string()
        }))
    }
}

impl<T> IntoResponse for anyhow::Result<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response<Body> {
        match self {
            Ok(data) => HttpRouterResult {
                success: true,
                code: 0,
                msg: "".to_string(),
                result: Some(data),
            }
            .into_response(),

            Err(err) => HttpRouterResult::<()> {
                success: false,
                code: -1,
                msg: err.to_string(),
                result: None,
            }
            .into_response(),
        }
    }
}

fn json_response(body: String) -> Response<Body> {
    let len = body.len();
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(http::header::CONTENT_LENGTH, HeaderValue::from(len));
    response
}

fn error_response(err: anyhow::Error) -> Response<Body> {
    HttpRouterResult::<()> {
        success: false,
        code: -1,
        msg: err.to_string(),
        result: None,
    }
    .into_response()
}

type RespBody = Response<Body>;

#[derive(Clone)]
pub struct Request {
    pub r: Option<Arc<HttpStreamRequest>>,
    pub method: Method,
    pub path: String,
    #[cfg(test)]
    test_body: Option<Arc<std::sync::Mutex<Option<Bytes>>>>,
}

impl Request {
    pub fn new(r: Arc<HttpStreamRequest>, method: Method, path: String) -> Self {
        Self {
            r: Some(r),
            method,
            path,
            #[cfg(test)]
            test_body: None,
        }
    }

    #[cfg(test)]
    pub fn new_test(method: Method, path: impl Into<String>, body: Option<Bytes>) -> Self {
        Self {
            r: None,
            method,
            path: path.into(),
            test_body: Some(Arc::new(std::sync::Mutex::new(body))),
        }
    }

    pub fn stream_request(&self) -> anyhow::Result<&Arc<HttpStreamRequest>> {
        self.r
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("http stream request is nil"))
    }

    pub async fn read_body(&self) -> anyhow::Result<Option<Bytes>> {
        #[cfg(test)]
        {
            if let Some(test_body) = &self.test_body {
                return Ok(test_body
                    .lock()
                    .map_err(|_| anyhow::anyhow!("test_body lock poisoned"))?
                    .take());
            }
        }

        let r = self.stream_request()?;
        let (body, content_length) = {
            let rctx = &mut *r.ctx.get_mut();
            if rctx.r_in.content_length > 0 {
                (rctx.r_in.body.take(), rctx.r_in.content_length)
            } else {
                (None, 0)
            }
        };

        if body.is_some() {
            let mut body = body.unwrap();
            use futures_util::StreamExt;
            let mut body_bytes = Vec::with_capacity(content_length as usize);
            loop {
                let data = body.next().await;
                if data.is_none() {
                    break;
                }
                let data = data.unwrap();
                if data.is_err() {
                    break;
                }
                let data = data.unwrap();
                if data.len() > 0 {
                    body_bytes.extend_from_slice(data.to_bytes().unwrap().as_ref());
                }
            }
            if body_bytes.is_empty() {
                Ok(None)
            } else {
                Ok(Some(bytes::Bytes::from(body_bytes)))
            }
        } else {
            Ok(None)
        }
    }
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "method:{}", self.method)?;
        write!(f, "path:{}", self.path)
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("path", &self.path)
            .finish()
    }
}

pub type Params = HashMap<String, String>;

#[derive(Debug, Clone)]
pub struct Json<T>(pub T);

pub trait HttpRouterState: Any + Send + Sync {
    fn clone_box(&self) -> Box<dyn HttpRouterState>;
    fn as_any(&self) -> &dyn Any;
}

impl<T> HttpRouterState for T
where
    T: Any + Clone + Send + Sync + 'static,
{
    fn clone_box(&self) -> Box<dyn HttpRouterState> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct HttpRouterStateBox(Box<dyn HttpRouterState>);

impl HttpRouterStateBox {
    pub fn new<S>(state: S) -> Self
    where
        S: Any + Clone + Send + Sync + 'static,
    {
        Self(Box::new(state))
    }

    pub fn as_any(&self) -> &dyn Any {
        self.0.as_any()
    }
}

impl Clone for HttpRouterStateBox {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

pub type AnyHttpRouter = HttpRouter<HttpRouterStateBox>;

type Handler<S> = Arc<dyn Send + Sync + Fn(Request, Params, S) -> BoxFuture<'static, RespBody>>;

pub trait FromHttpRouterState: Sized {
    fn from_http_router_state(state: HttpRouterStateBox) -> anyhow::Result<Self>;
}

impl<T> FromHttpRouterState for T
where
    T: Any + Clone + Send + Sync + 'static,
{
    fn from_http_router_state(state: HttpRouterStateBox) -> anyhow::Result<Self> {
        state.as_any().downcast_ref::<T>().cloned().ok_or_else(|| {
            anyhow::anyhow!("router state type is not {}", std::any::type_name::<T>())
        })
    }
}

pub struct HttpRouter<S = ()> {
    routes: HashMap<Method, Router<Handler<S>>>,
    state: S,
}

impl HttpRouter<()> {
    pub fn new() -> Self {
        Self::with_state(())
    }
}

impl<S> HttpRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn with_state(state: S) -> Self {
        Self {
            routes: HashMap::new(),
            state,
        }
    }
}

impl AnyHttpRouter {
    pub fn with_any_state<S>(state: S) -> Self
    where
        S: Any + Clone + Send + Sync + 'static,
    {
        Self::with_state(HttpRouterStateBox::new(state))
    }

    pub fn add_json<F, Fut, R, B, T>(&mut self, method: Method, path: &str, handler: F)
    where
        F: Fn(Request, Params, Json<B>, T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: IntoResponse + 'static,
        B: Default + DeserializeOwned + Send + 'static,
        T: FromHttpRouterState + Send + 'static,
    {
        let handler = Arc::new(handler);
        let h: Handler<HttpRouterStateBox> = Arc::new(move |req, params, state| {
            let handler = handler.clone();
            async move {
                let body = match req.read_body().await {
                    Ok(Some(body)) => body,
                    Ok(None) => {
                        let state = match T::from_http_router_state(state) {
                            Ok(state) => state,
                            Err(err) => return error_response(err),
                        };
                        return handler(req, params, Json(B::default()), state)
                            .await
                            .into_response();
                    }
                    Err(err) => return error_response(err),
                };
                let body = match serde_json::from_slice::<B>(body.as_ref()) {
                    Ok(body) => body,
                    Err(err) => {
                        return error_response(anyhow::anyhow!(
                            "serde_json::from_slice err:{:?}",
                            err
                        ))
                    }
                };
                let state = match T::from_http_router_state(state) {
                    Ok(state) => state,
                    Err(err) => return error_response(err),
                };

                handler(req, params, Json(body), state)
                    .await
                    .into_response()
            }
            .boxed()
        });

        self.routes
            .entry(method)
            .or_insert_with(Router::new)
            .insert(path, h)
            .unwrap();
    }

    pub fn add<F, Fut, T>(&mut self, method: Method, path: &str, handler: F)
    where
        F: Fn(Request, Params, T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<Response<Body>>> + Send + 'static,
        T: FromHttpRouterState + Send + 'static,
    {
        let handler = Arc::new(handler);
        let h: Handler<HttpRouterStateBox> = Arc::new(move |req, params, state| {
            let handler = handler.clone();
            async move {
                let state = match T::from_http_router_state(state) {
                    Ok(state) => state,
                    Err(err) => return error_response(err),
                };
                match handler(req, params, state).await {
                    Ok(response) => response,
                    Err(err) => error_response(err),
                }
            }
            .boxed()
        });

        self.routes
            .entry(method)
            .or_insert_with(Router::new)
            .insert(path, h)
            .unwrap();
    }
}

impl<S> HttpRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn state(&self) -> S {
        self.state.clone()
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub async fn call(&self, req: Request) -> Response<Body> {
        let Some(router) = self.routes.get(&req.method) else {
            log::debug!("not_found router_req:{:?}", req);
            return not_found()
                .map_err(|e| anyhow::anyhow!("self.routes.get err:{:?}", e))
                .into_response();
        };

        let Ok(matched) = router.at(&req.path) else {
            log::debug!("not_found router_req:{:?}", req);
            return not_found()
                .map_err(|e| anyhow::anyhow!("router.at err:{:?}", e))
                .into_response();
        };

        let mut params = HashMap::new();

        for (k, v) in matched.params.iter() {
            params.insert(k.to_string(), v.to_string());
        }

        let handler = matched.value.clone();

        handler(req, params, self.state.clone()).await
    }
}

fn not_found() -> anyhow::Result<String> {
    Err(anyhow::anyhow!("404 not found"))
}

//curl http://www.example.cn:11210/test2/hello -k -v
//curl http://www.example.cn:11210/test2/echo -k -v
//curl http://www.example.cn:11210/test2/ok -k -v
//curl -X POST -H "Content-Type: application/json" -d '{"id":"123"}' http://www.example.cn:11210/test2/user/123 -k -v
//curl -X POST -H "Content-Type: application/json" -d '{"id":"123"}' http://www.example.cn:11210/test2/ok_hyper -k -v
pub async fn router_handle_anyproxy_test(_name: String) -> anyhow::Result<Option<AnyHttpRouter>> {
    let mut router = AnyHttpRouter::with_any_state(Arc::new(AnyProxyRouterTestData::new(77)));
    router.add_json(Method::GET, "/test2/hello", hello_test_2);
    router.add_json(Method::GET, "/test2/echo", echo_test_2);
    router.add_json(Method::POST, "/test2/user/{id}", user_test_2);
    router.add_json(Method::GET, "/test2/ok", ok_test_2);
    router.add(Method::POST, "/test2/ok_hyper", ok_hyper_test_2);
    return Ok(Some(router));
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

#[derive(Clone)]
pub struct AnyProxyRouterTestData {
    num: i32,
}

impl AnyProxyRouterTestData {
    pub fn new(num: i32) -> Self {
        AnyProxyRouterTestData { num }
    }
}

pub async fn echo_test_2(
    req: Request,
    params: Params,
    _json: Json<()>,
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
    req: Request,
    params: Params,
    Json(req_data): Json<UserTestDataReq>,
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
    req: Request,
    params: Params,
    _json: Json<()>,
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
    req: Request,
    params: Params,
    _json: Json<()>,
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
    req: Request,
    params: Params,
    state: Arc<AnyProxyRouterTestData>,
) -> anyhow::Result<hyper::Response<hyper::Body>> {
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
    let mut response = hyper::Response::new(hyper::Body::from(resp_body));
    response.headers_mut().insert(
        http::header::CONTENT_LENGTH,
        HeaderValue::from(resp_body_len),
    );
    Ok(response)
}
