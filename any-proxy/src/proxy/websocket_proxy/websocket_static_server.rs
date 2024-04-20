use super::WEBSOCKET_HELLO_KEY;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ShareRw};
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use futures_util::{SinkExt, StreamExt};
use std::io::Read;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::WebSocketStream;

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );
    WebsocketServer::new(arg).run(client_buf_stream).await
}

pub struct WebsocketServer {
    arg: ServerArg,
}

impl WebsocketServer {
    pub fn new(arg: ServerArg) -> WebsocketServer {
        WebsocketServer { arg }
    }

    pub async fn run(&mut self, stream: BufStream<StreamFlow>) -> Result<()> {
        let mut err = None;
        let mut ws_host = "".to_string();
        let mut proxy_hello: Option<(AnyproxyHello, usize)> = None;
        let mut ups_request: Request = Request::new(());
        let mut name = "".to_string();

        if self.arg.server_stream_info.is_tls {
            self.arg.stream_info.get_mut().client_protocol77 = Some(Protocol77::WebSockets);
        } else {
            self.arg.stream_info.get_mut().client_protocol77 = Some(Protocol77::WebSocket);
        }

        let copy_headers_callback = |request: &Request,
                                     response: Response|
         -> std::result::Result<Response, ErrorResponse> {
            for (k, v) in request.headers().iter() {
                ups_request.headers_mut().insert(k, v.clone());
            }
            log::debug!("request.headers:{:?}", request.headers());
            name = request.uri().path().to_string();
            let host_func = |request: &Request| -> Result<String> {
                let host = request
                    .headers()
                    .get(http::header::HOST)
                    .ok_or(anyhow::anyhow!("err:host nil"))?
                    .to_str()
                    .map_err(|e| anyhow::anyhow!("err:host => e:{}", e))?
                    .to_string();
                Ok(host)
            };
            let host = host_func(request);
            match host {
                Ok(host) => {
                    ws_host = host;
                }
                Err(e) => {
                    err = Some(anyhow!("err:websocket => e:{}", e));
                    return Ok(response);
                }
            }

            let hello_func = |request: &Request| -> Option<String> {
                let hello = request.headers().get(&*WEBSOCKET_HELLO_KEY);
                if hello.is_none() {
                    return None;
                }
                let hello = hello.unwrap();
                match hello.to_str() {
                    Ok(hello) => Some(hello.to_string()),
                    Err(_) => None,
                }
            };
            let hello = hello_func(request);
            if hello.is_some() {
                let hello =
                    match general_purpose::STANDARD.decode(hello.as_ref().unwrap().as_bytes()) {
                        Ok(hello) => hello,
                        Err(e) => {
                            err = Some(anyhow!("err:websocket => e:{}", e));
                            return Ok(response);
                        }
                    };
                let hello: Result<AnyproxyHello> =
                    toml::from_slice(&hello).map_err(|e| anyhow!("err:toml::from_slice=> e:{}", e));
                if let Ok(hello) = hello {
                    proxy_hello = Some((hello, 0))
                }
            }

            Ok(response)
        };

        let client_stream = tokio_tungstenite::accept_hdr_async(stream, copy_headers_callback)
            .await
            .map_err(|e| anyhow::anyhow!("err:accept_hdr_async => e:{}", e))?;

        if err.is_some() {
            return Err(err.unwrap());
        }
        log::debug!("ws_host:{}", ws_host);
        let (domain, _) = host_and_port(&ws_host);
        let domain = ArcString::new(domain.to_string());

        let scc = proxy_util::parse_proxy_domain(
            &self.arg,
            move || async {
                let hello = proxy_hello;
                Ok(hello)
            },
            || async { Ok(domain) },
        )
        .await?;

        self.steam_to_stream(client_stream, scc, name).await
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        scc: ShareRw<StreamConfigContext>,
        mut name: String,
    ) -> Result<()> {
        let file_name = {
            let scc = scc.get();
            use crate::config::http_server_static_websocket;
            let http_server_static_websocket_conf =
                http_server_static_websocket::currs_conf(scc.http_server_confs());
            let mut seq = "";
            log::trace!("name:{}", name);
            if name.len() <= 0 || name == "/" {
                seq = "/";
                name = http_server_static_websocket_conf.conf.index.clone();
            }

            let file_name = format!(
                "{}{}{}",
                http_server_static_websocket_conf.conf.path, seq, name
            );
            file_name
        };
        let (mut w, _r) = client_stream.split();

        let file = std::fs::File::open(&file_name)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;
        let file = ArcMutex::new(file);

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
                w.send(buffer.into()).await?;
                let _ = w.flush().await;
            }
            if size != buffer_len {
                break;
            }
        }

        Ok(())
    }
}
