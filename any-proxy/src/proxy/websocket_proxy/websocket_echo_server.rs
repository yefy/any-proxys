use crate::proxy::stream_info::ErrStatus;
use crate::proxy::websocket_proxy::stream_parse;
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
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
        let (_, scc, client_stream) = value.unwrap();

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
        let _ = w.flush().await;
        Ok(())
    }
}
