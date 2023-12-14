use super::http_server;
use crate::proxy::{ServerArg, StreamConfigContext};
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use hyper::body::Bytes;
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;
use std::io::Read;

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );
    http_server::http_connection(arg, client_buf_stream, |arg, http_arg, scc, req| {
        Box::pin(http_server_run_handle(arg, http_arg, scc, req))
    })
    .await
}

pub async fn http_server_run_handle(
    arg: ServerArg,
    _http_arg: ServerArg,
    scc: ShareRw<StreamConfigContext>,
    req: Request<Body>,
) -> Result<Response<Body>> {
    HttpStaticServer::new(arg.executors.clone(), req, scc)
        .run()
        .await
}

pub struct HttpStaticServer {
    executors: ExecutorsLocal,
    req: Request<Body>,
    scc: ShareRw<StreamConfigContext>,
}

impl HttpStaticServer {
    pub fn new(
        executors: ExecutorsLocal,
        req: Request<Body>,
        scc: ShareRw<StreamConfigContext>,
    ) -> HttpStaticServer {
        HttpStaticServer {
            executors,
            req,
            scc,
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
        log::trace!("self.req.version:{:?}", self.req.version());
        log::trace!("self.req:{:?}", self.req);
        /*
        use hyper::body::HttpBody;
        let body = self.req.data().await;
        if body.is_none() {
            log::trace!("body.is_none()");
        } else {
            let body = body.unwrap();
            if body.is_err() {
                log::trace!("body.is_err()");
            } else {
                let body = body.unwrap();
                log::trace!("body.len:{}", body.len());
            }
        }
         */

        let scc = self.scc.get();
        use crate::config::http_server_static;
        let http_server_static_conf = http_server_static::currs_conf(scc.http_server_confs());
        let mut seq = "";
        let mut name = self.req.uri().path();

        log::trace!("name:{}", name);
        if name.len() <= 0 || name == "/" {
            seq = "/";
            name = &http_server_static_conf.conf.index;
        }

        let file_name = format!("{}{}{}", http_server_static_conf.conf.path, seq, name);
        let file = std::fs::File::open(&file_name)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;
        let file_len = file
            .metadata()
            .map_err(|e| anyhow!("err:file.metadata => file_name:{}, e:{}", file_name, e))?
            .len()
            .to_string();
        let file = ArcMutex::new(file);

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
                    let file = file.clone();
                    let data: Result<(usize, Vec<u8>)> = tokio::task::spawn_blocking(move || {
                        let mut buffer = vec![0u8; buffer_len];
                        let size = file
                            .get_mut()
                            .read(&mut buffer.as_mut_slice()[..])
                            .map_err(|e| anyhow!("err:file.read => e:{}", e))?;
                        Ok((size, buffer))
                    })
                    .await?;
                    let (size, mut buffer) = data?;

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
