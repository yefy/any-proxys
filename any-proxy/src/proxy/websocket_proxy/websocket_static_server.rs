use crate::proxy::stream_info::ErrStatus;
use crate::proxy::websocket_proxy::stream_parse;
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::{StreamFlow, StreamFlowErr};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::io::Read;
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
        let (r, scc, client_stream) = value.unwrap();
        let path = r.ctx.get().r_in.uri.path().to_string();

        self.arg.stream_info.get_mut().err_status = ErrStatus::Ok;
        self.steam_to_stream(client_stream, scc, path).await
    }

    pub async fn steam_to_stream(
        &self,
        client_stream: WebSocketStream<BufStream<StreamFlow>>,
        scc: Arc<StreamConfigContext>,
        mut path: String,
    ) -> Result<()> {
        let file_name = {
            use crate::config::net_server_static_websocket;
            let http_server_static_websocket_conf =
                net_server_static_websocket::curr_conf(scc.net_curr_conf());
            let mut seq = "";
            log::trace!(target: "main", "path:{}", path);
            if path.len() <= 0 || path == "/" {
                seq = "/";
                path = http_server_static_websocket_conf.conf.index.clone();
            }

            let file_name = format!(
                "{}{}{}",
                http_server_static_websocket_conf.conf.path, seq, path
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

        use chrono::Local;
        let stream_info = self.arg.stream_info.get();
        stream_info.client_stream_flow_info.get_mut().err = StreamFlowErr::WriteClose;
        stream_info
            .client_stream_flow_info
            .get_mut()
            .err_time_millis = Local::now().timestamp_millis();

        Ok(())
    }
}
