use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::typ::ArcMutex;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio_tungstenite::WebSocketStream;

pub async fn server_handle(
    arg: ServerArg,
    client_stream: ArcMutex<WebSocketStream<BufStream<StreamFlow>>>,
) -> Result<crate::Error> {
    let scc = arg.stream_info.get().scc.clone();
    use crate::config::net_server_websocket_echo;
    let net_server_websocket_echo_conf =
        net_server_websocket_echo::curr_conf_mut(scc.net_curr_conf());
    if net_server_websocket_echo_conf.body.is_none() {
        return Ok(crate::Error::Ok);
    }

    let client_stream = unsafe { client_stream.take().unwrap() };
    WebsocketServer::new(arg).run(client_stream).await?;

    return Ok(crate::Error::Finish);
}

pub struct WebsocketServer {
    arg: ServerArg,
}

impl WebsocketServer {
    pub fn new(arg: ServerArg) -> WebsocketServer {
        WebsocketServer { arg }
    }

    pub async fn run(
        &mut self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
    ) -> Result<()> {
        let scc = self.arg.stream_info.get().scc.clone();
        self.steam_to_stream(client_stream, &scc).await
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        scc: &Arc<StreamConfigContext>,
    ) -> Result<()> {
        use crate::config::net_server_websocket_echo;
        let net_server_websocket_echo_conf =
            net_server_websocket_echo::curr_conf(scc.net_curr_conf());
        let (mut w, _r) = client_stream.split();
        w.send(net_server_websocket_echo_conf.body.clone().unwrap().into())
            .await?;
        let _ = w.flush().await;
        Ok(())
    }
}
