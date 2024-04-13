use super::WEBSOCKET_HELLO_KEY;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
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

        self.steam_to_stream(client_stream, scc).await
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        scc: Arc<StreamConfigContext>,
    ) -> Result<()> {
        use crate::config::net_server_echo_websocket;
        let http_server_echo_websocket_conf =
            net_server_echo_websocket::curr_conf(scc.net_curr_conf());
        let (mut w, _r) = client_stream.split();
        w.send(http_server_echo_websocket_conf.body.clone().into())
            .await?;
        Ok(())
    }
}
