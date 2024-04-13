use crate::proxy::http_proxy::http_connection;
use crate::proxy::{ServerArg, StreamConfigContext};
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;
use anyhow::Result;
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;
use std::sync::Arc;

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );
    http_connection(arg, client_buf_stream, |arg, http_arg, scc, request| {
        Box::pin(http_server_run_handle(arg, http_arg, scc, request))
    })
    .await
}

pub async fn http_server_run_handle(
    arg: ServerArg,
    _http_arg: ServerArg,
    scc: Arc<StreamConfigContext>,
    request: Request<Body>,
) -> Result<Response<Body>> {
    HttpEchoServer::new(arg.executors.clone(), request, scc)
        .run()
        .await
}

pub struct HttpEchoServer {
    _executors: ExecutorsLocal,
    request: Request<Body>,
    scc: Arc<StreamConfigContext>,
}

impl HttpEchoServer {
    pub fn new(
        executors: ExecutorsLocal,
        request: Request<Body>,
        scc: Arc<StreamConfigContext>,
    ) -> HttpEchoServer {
        HttpEchoServer {
            _executors: executors,
            request,
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
        let scc = self.scc.clone();
        use crate::config::net_server_echo_http;
        let http_server_echo_conf = net_server_echo_http::curr_conf(scc.net_curr_conf());

        let mut response = Response::new(http_server_echo_conf.body.clone().into());
        response.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v1.0.0".as_bytes())?,
        );
        response.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        response.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes("Thu, 08 Jul 2021 09:30:53 GMT".as_bytes())?,
        );
        response.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        response
            .headers_mut()
            .insert("ETag", HeaderValue::from_bytes("60e6c5cd-e".as_bytes())?);
        response.headers_mut().insert(
            "Accept-Ranges",
            HeaderValue::from_bytes("bytes".as_bytes())?,
        );
        if *self.request.version_mut() == Version::HTTP_2 {
            response.headers_mut().remove("connection");
        }

        if *self.request.version_mut() == Version::HTTP_10 {
            response.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        *response.version_mut() = *self.request.version_mut();

        Ok(response)
    }
}
