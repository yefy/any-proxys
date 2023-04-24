use any_base::executor_local_spawn::ExecutorsLocal;
use anyhow::Result;
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;

pub struct HttpEchoServer<'a> {
    _executors: ExecutorsLocal,
    req: Request<Body>,
    body: &'a str,
}

impl HttpEchoServer<'_> {
    pub fn new(executors: ExecutorsLocal, req: Request<Body>, body: &str) -> HttpEchoServer {
        HttpEchoServer {
            _executors: executors,
            req,
            body,
        }
    }

    pub async fn run(&mut self) -> Result<Response<Body>> {
        let ret = self.do_run().await;
        if let Err(e) = ret {
            log::error!("err:run => e:{}", e);
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::default())?);
        }
        ret
    }

    pub async fn do_run(&mut self) -> Result<Response<Body>> {
        let mut res = Response::new(self.body.to_string().into());
        res.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v1.0.0".as_bytes())?,
        );
        res.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        res.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes("Thu, 08 Jul 2021 09:30:53 GMT".as_bytes())?,
        );
        res.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        res.headers_mut()
            .insert("ETag", HeaderValue::from_bytes("60e6c5cd-e".as_bytes())?);
        res.headers_mut().insert(
            "Accept-Ranges",
            HeaderValue::from_bytes("bytes".as_bytes())?,
        );
        if *self.req.version_mut() == Version::HTTP_2 {
            res.headers_mut().remove("connection");
        }

        if *self.req.version_mut() == Version::HTTP_10 {
            res.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        *res.version_mut() = *self.req.version_mut();

        Ok(res)
    }
}
