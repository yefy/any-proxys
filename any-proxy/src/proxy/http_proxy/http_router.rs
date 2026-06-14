use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::Method;
use log;
use matchit::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

pub trait IntoResponse {
    fn into_response(self) -> String;
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
    fn into_response(self) -> String {
        serde_json::to_string(&self).unwrap_or_else(|_| {
            r#"{"success":false,"code":-1,"msg":"json serialize error","result":null}"#.to_string()
        })
    }
}

impl<T> IntoResponse for anyhow::Result<T>
where
    T: Serialize,
{
    fn into_response(self) -> String {
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

type RespBody = String;

#[derive(Clone)]
pub struct Request {
    pub r: Arc<HttpStreamRequest>,
    pub method: Method,
    pub path: String,
    pub body: Option<bytes::Bytes>,
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = if self.body.is_none() {
            None
        } else {
            Some(String::from_utf8_lossy(self.body.as_ref().unwrap()))
        };

        write!(f, "method:{}", self.method)?;
        write!(f, "path:{}", self.path)?;
        write!(f, "body:{:?}", body)
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = if self.body.is_none() {
            None
        } else {
            Some(String::from_utf8_lossy(self.body.as_ref().unwrap()))
        };
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("path", &body)
            .finish()
    }
}

pub type Params = HashMap<String, String>;

type Handler = Arc<dyn Send + Sync + Fn(Request, Params) -> BoxFuture<'static, RespBody>>;

pub struct HttpRouter {
    routes: HashMap<Method, Router<Handler>>,
}

impl HttpRouter {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn add<F, Fut, R>(&mut self, method: Method, path: &str, handler: F)
    where
        F: Fn(Request, Params) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: IntoResponse + 'static,
    {
        let handler = Arc::new(handler);
        let h: Handler = Arc::new(move |req, params| {
            let handler = handler.clone();
            async move { handler(req, params).await.into_response() }.boxed()
        });

        self.routes
            .entry(method)
            .or_insert_with(Router::new)
            .insert(path, h)
            .unwrap();
    }

    pub async fn call(&self, req: Request) -> String {
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

        handler(req, params).await
    }
}

fn not_found() -> anyhow::Result<String> {
    Err(anyhow::anyhow!("404 not found"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoTestData {
    pub data: String,
}

pub async fn echo_test(req: Request, params: Params) -> anyhow::Result<EchoTestData> {
    log::info!("hello_test req:{:?}, params:{:?}", req, params);
    if req.body.is_some() {
        return Err(anyhow::anyhow!("body not nil:{:?}", req.body));
    }
    Ok(EchoTestData {
        data: "echo_test".to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTestData {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTestDataReq {
    pub id: String,
}

pub async fn user_test(req: Request, params: Params) -> anyhow::Result<UserTestData> {
    log::info!("hello_test req:{:?}, params:{:?}", req, params);
    if req.body.is_none() {
        return Err(anyhow::anyhow!("body is nil:{:?}", req.body));
    }

    let req_data: UserTestDataReq = serde_json::from_slice(req.body.as_ref().unwrap())
        .map_err(|e| anyhow::anyhow!("serde_json::from_str err:{:?}", e))?;
    log::info!("req_data:{:?}", req_data);

    let id = params.get("id").cloned().unwrap_or_default();

    if req_data.id != id {
        return Err(anyhow::anyhow!("req_data.id:{} != id:{}", req_data.id, id));
    }

    Ok(UserTestData {
        id,
        name: "张三".to_string(),
    })
}

pub async fn hello_test(req: Request, params: Params) -> anyhow::Result<String> {
    log::info!("hello_test req:{:?}, params:{:?}", req, params);
    if req.body.is_some() {
        return Err(anyhow::anyhow!("body not nil:{:?}", req.body));
    }
    Ok("hello world".to_string())
}

pub async fn ok_test(req: Request, params: Params) -> anyhow::Result<()> {
    log::info!("hello_test req:{:?}, params:{:?}", req, params);
    if req.body.is_some() {
        return Err(anyhow::anyhow!("body not nil:{:?}", req.body));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_router() {
        let mut router = HttpRouter::new();

        router.add(Method::GET, "/test/hello", hello_test);
        router.add(Method::GET, "/test/echo", echo_test);
        router.add(Method::POST, "/test/user/{id}", user_test);
        router.add(Method::GET, "/test/ok", ok_test);

        let req = Request {
            method: Method::GET,
            path: "/test/hello".to_string(),
            body: None,
        };

        let resp = router.call(req).await;

        println!("resp1 = {}", resp);

        let req = Request {
            method: Method::GET,
            path: "/test/echo".to_string(),
            body: None,
        };

        let resp = router.call(req).await;

        println!("resp2 = {}", resp);

        let req = Request {
            method: Method::POST,
            path: "/test/user/123".to_string(),
            body: Some(r#"{"id":"123"}"#.to_string()),
        };

        let resp = router.call(req).await;

        println!("resp3 = {}", resp);

        let req = Request {
            method: Method::GET,
            path: "/not_found".to_string(),
            body: None,
        };

        let resp = router.call(req).await;

        println!("resp4 = {}", resp);

        let req = Request {
            method: Method::GET,
            path: "/test/ok".to_string(),
            body: None,
        };

        let resp = router.call(req).await;

        println!("resp5 = {}", resp);
    }
}
