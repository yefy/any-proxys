use super::WEBSOCKET_HELLO_KEY;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::util as proxy_util;
use crate::proxy::websocket_proxy::stream_parse;
use crate::proxy::ServerArg;
use crate::Protocol77;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use futures_core::stream::Stream;
use futures_util::{SinkExt, StreamExt};
use http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::error::Error;
use tokio_tungstenite::tungstenite::handshake::server::Request;
use tokio_tungstenite::tungstenite::Message;
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
        self.arg.stream_info.get_mut().err_status = ErrStatus::ServiceUnavailable;
        let value = stream_parse(self.arg.clone(), stream).await?;
        if value.is_none() {
            return Ok(());
        }
        let (r, scc, client_stream) = value.unwrap();

        let (is_proxy_protocol_hello, connect_func) =
            proxy_util::upsteam_connect_info(self.arg.stream_info.clone(), scc.clone()).await?;
        let upstream_is_tls = connect_func.is_tls().await;
        if upstream_is_tls {
            self.arg.stream_info.get_mut().upstream_protocol77 = Some(Protocol77::WebSockets);
        } else {
            self.arg.stream_info.get_mut().upstream_protocol77 = Some(Protocol77::WebSocket);
        }
        let upstream_host = connect_func.host().await?;
        let upstream_stream =
            proxy_util::upsteam_do_connect(self.arg.stream_info.clone(), connect_func)
                .await
                .map_err(|e| anyhow!("err:upsteam_do_connect => e:{}", e))?;

        let hello = proxy_util::get_proxy_hello(
            is_proxy_protocol_hello,
            self.arg.stream_info.clone(),
            scc.clone(),
        )
        .await;

        let upstream_scheme = if upstream_is_tls { "wss" } else { "ws" };
        let url_string = {
            let r_ctx = r.ctx.get();
            let req_uri = r_ctx
                .r_in
                .uri
                .path_and_query()
                .map(|x| x.as_str())
                .unwrap_or("/");
            format!("{}://{}{}", upstream_scheme, upstream_host, req_uri)
        };
        log::trace!(target: "main", "url_string = {}", url_string);
        let uri = url_string
            .parse()
            .map_err(|e| anyhow!("err:parse => e:{}", e))?;

        r.ctx.get_mut().r_in.uri_upstream = uri;

        if hello.is_some() {
            let hello_str = toml::to_string(&*hello.unwrap())?;
            let hello_str = general_purpose::STANDARD.encode(hello_str);
            self.arg.stream_info.get_mut().upstream_protocol_hello_size = hello_str.len();
            let key = HeaderName::from_bytes(WEBSOCKET_HELLO_KEY.as_bytes())?;
            let value = HeaderValue::from_bytes(hello_str.as_bytes())?;
            r.ctx.get_mut().r_in.headers_upstream.insert(key, value);
        }

        let mut ups_request = Request::new(());
        {
            let r_in = &r.ctx.get().r_in;
            *ups_request.method_mut() = r_in.method_upstream.clone();
            *ups_request.version_mut() = r_in.version_upstream.clone();
            *ups_request.uri_mut() = r_in.uri_upstream.clone();
            *ups_request.headers_mut() = r_in.headers_upstream.clone();
        }
        //___wait___
        //这样会失败
        // let upstream_stream =
        //     any_base::io::buf_stream::BufStream::from(any_base::io::buf_writer::BufWriter::new(
        //         any_base::io::buf_reader::BufReader::new(upstream_stream),
        //     ));
        let (upstream_stream, _) =
            tokio_tungstenite::client_async_with_config(ups_request, upstream_stream, None)
                .await
                .map_err(|e| anyhow!("err:client_async_with_config => e:{}", e))?;

        self.arg.stream_info.get_mut().err_status = ErrStatus::Ok;
        self.steam_to_stream(client_stream, upstream_stream)
            .await
            .map_err(|e| anyhow!("err:steam_to_stream => e:{}", e))
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        upstream_stream: WebSocketStream<StreamFlow>,
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

    // pub async fn do_steam_to_stream(
    //     mut r: SplitStream<WebSocketStream<BufStream<StreamFlow>>>,
    //     mut w: SplitSink<WebSocketStream<BufStream<StreamFlow>>, Message>,
    // ) -> Result<()> {
    pub async fn do_steam_to_stream<R, W>(mut r: R, mut w: W) -> Result<()>
    where
        W: SinkExt<Message> + std::marker::Unpin,
        R: Stream<Item = Result<Message, Error>> + std::marker::Unpin,
        <W as futures_util::Sink<tokio_tungstenite::tungstenite::Message>>::Error:
            std::fmt::Display,
    {
        loop {
            let msg = r.next().await;
            match msg {
                Some(msg) => {
                    let msg = msg?;
                    match w.send(msg).await {
                        Ok(()) => {
                            let _ = w.flush().await;
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("err:{}", e));
                        }
                    }
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
