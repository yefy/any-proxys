use any_base::executor_local_spawn::ExecutorsLocal;
use anyhow::anyhow;
use anyhow::Result;
use hyper::body::Bytes;
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;
use std::io::Read;

pub struct HttpStaticServer<'a> {
    executors: ExecutorsLocal,
    req: Request<Body>,
    path: &'a str,
}

impl HttpStaticServer<'_> {
    pub fn new(executors: ExecutorsLocal, req: Request<Body>, path: &str) -> HttpStaticServer {
        HttpStaticServer {
            executors,
            req,
            path,
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
        let file_name = format!("{}{}", self.path, self.req.uri().path().to_string());
        let mut file = std::fs::File::open(&file_name)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;
        let file_len = file
            .metadata()
            .map_err(|e| anyhow!("err:file.metadata => file_name:{}, e:{}", file_name, e))?
            .len()
            .to_string();

        let (mut res_sender, res_body) = Body::channel();
        let mut res = Response::new(res_body);
        res.headers_mut().insert(
            "Content-Length",
            HeaderValue::from_bytes(file_len.as_bytes())?,
        );
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

        self.executors._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
            move |_| async move {
                let buffer_len = 8192;
                loop {
                    let mut buffer = vec![0u8; buffer_len];
                    let size = file
                        .read(&mut buffer.as_mut_slice()[..])
                        .map_err(|e| anyhow!("err:file.read => e:{}", e))?;
                    if size > 0 {
                        if size != buffer_len {
                            unsafe { buffer.set_len(size) }
                        }
                        res_sender.send_data(Bytes::from(buffer)).await?;
                    }
                    if size != buffer_len {
                        break;
                    }
                }
                Ok(())
            },
        );

        Ok(res)
    }
}
