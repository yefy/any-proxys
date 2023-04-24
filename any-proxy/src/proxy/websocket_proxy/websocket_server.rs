use super::WEBSOCKET_HELLO_KEY;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::util as proxy_util;
use crate::proxy::ServerArg;
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use anyhow::anyhow;
use anyhow::Result;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

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

        self.arg.stream_info.borrow_mut().protocol77 = Some(Protocol77::WebSocket);

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
                let hello: Result<AnyproxyHello> =
                    toml::from_slice(hello.as_ref().unwrap().as_bytes())
                        .map_err(|e| anyhow!("err:toml::from_slice=> e:{}", e));
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

        let stream_config_context = proxy_util::parse_proxy_domain(
            &self.arg,
            move || async {
                let hello = proxy_hello;
                Ok(hello)
            },
            || async { Ok(domain.to_string()) },
        )
        .await?;

        if stream_config_context.server.is_none() {
            return Err(anyhow!("err:stream_config_context.server.is_none"));
        }

        let (is_proxy_protocol_hello, upstream_stream) =
            proxy_util::upsteam_connect(self.arg.stream_info.clone(), &stream_config_context)
                .await?;

        let upstream_stream =
            any_base::io::buf_stream::BufStream::from(any_base::io::buf_writer::BufWriter::new(
                any_base::io::buf_reader::BufReader::new(upstream_stream),
            ));

        let hello = proxy_util::get_proxy_hello(
            is_proxy_protocol_hello,
            self.arg.stream_info.clone(),
            &stream_config_context,
        )
        .await;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            self.arg.stream_info.borrow_mut().protocol_hello_size = hello_str.len();
            ups_request.headers_mut().insert(
                HeaderName::from_bytes(WEBSOCKET_HELLO_KEY.as_bytes())?,
                HeaderValue::from_bytes(hello_str.as_bytes())?,
            );
        }

        let (upstream_stream, _) =
            tokio_tungstenite::client_async_with_config(ups_request, upstream_stream, None).await?;

        self.steam_to_stream(client_stream, upstream_stream).await
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        upstream_stream: WebSocketStream<BufStream<StreamFlow>>,
    ) -> Result<()> {
        let (client_w, client_r) = client_stream.split();
        let (ups_w, ups_r) = upstream_stream.split();

        let ret: Result<()> = async {
            tokio::select! {
                ret = WebsocketServer::do_steam_to_stream(
                    client_r,
                    ups_w,
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                ret =  WebsocketServer::do_steam_to_stream(
                    ups_r,
                    client_w,
                ) => {
                    return ret.map_err(|e| anyhow!("err:ups => e:{}", e));
                }
                else => {
                    return Err(anyhow!("err:steam_to_stream select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn do_steam_to_stream(
        mut r: SplitStream<WebSocketStream<BufStream<StreamFlow>>>,
        mut w: SplitSink<WebSocketStream<BufStream<StreamFlow>>, Message>,
    ) -> Result<()> {
        loop {
            let msg = r.next().await;
            match msg {
                Some(msg) => {
                    let msg = msg?;
                    w.send(msg).await?;
                    // if msg.is_text() || msg.is_binary() {
                    //     w.send(msg).await?;
                    // } else if msg.is_close() {
                    //     return Ok(());
                    // }
                }
                None => return Ok(()),
            }
        }
    }
}
