use crate::wasm::component::server::wasm_http;
use crate::wasm::component::server::wasm_socket;
use crate::wasm::component::server::wasm_websocket;
use crate::wasm::WasmHost;
use crate::wasm::{socket_connect, WasmStreamInfo};
use any_base::typ::{ArcMutexTokio, Share};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::Ordering;

use crate::proxy::websocket_proxy::WebSocketStreamTrait;
use futures_util::{SinkExt, StreamExt};

impl TryFrom<wasm_http::Request> for http::Request<()> {
    type Error = anyhow::Error;

    fn try_from(wasm_req: wasm_http::Request) -> Result<Self, Self::Error> {
        use super::wasm_host_http::method_to_str;
        use std::str::FromStr;
        let mut http_req = http::Request::builder()
            .method(http::Method::from_str(method_to_str(&wasm_req.method))?)
            .uri(&wasm_req.uri);

        for (key, value) in wasm_req.headers {
            http_req = http_req.header(key, value);
        }

        Ok(http_req.body(())?)
    }
}

impl WasmHost {
    pub fn get_websocket_stream(
        &self,
        fd: &u64,
    ) -> Option<(
        ArcMutexTokio<Box<dyn WebSocketStreamTrait>>,
        Share<WasmStreamInfo>,
    )> {
        let stream = self
            .wasm_stream_info
            .get_mut()
            .wasm_websocket_map
            .get(fd)
            .cloned();
        let stream = if stream.is_none() {
            let stream_info = self.wasm_stream_info_map.get().get(&fd).cloned();
            if stream_info.is_none() {
                return None;
            }
            let stream_info = stream_info.unwrap();

            let stream = stream_info.get_mut().wasm_websocket_map.get(fd).cloned();
            if stream.is_none() {
                return None;
            }
            Some((stream.unwrap(), stream_info.clone()))
        } else {
            Some((stream.unwrap(), self.wasm_stream_info.clone()))
        };
        stream
    }
}

#[async_trait]
impl wasm_websocket::Host for WasmHost {
    async fn socket_connect(
        &mut self,
        typ: wasm_socket::SocketType,
        req: wasm_http::Request,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let ret: Result<u64> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let http_req: http::Request<()> = match req.try_into() {
                Ok(r) => r,
                Err(_) => {
                    return Ok(0);
                }
            };
            let domain = http_req.uri().host();
            if domain.is_none() {
                return Err(anyhow::anyhow!("domain.is_none"));
            }
            let domain = domain.unwrap();
            let port = http_req.uri().port_u16();
            let http_host = if port.is_none() {
                match typ {
                    wasm_socket::SocketType::Tcp => {
                        format!("{}:80", domain)
                    }
                    wasm_socket::SocketType::Ssl => {
                        format!("{}:443", domain)
                    }
                    wasm_socket::SocketType::Quic => {
                        format!("{}:443", domain)
                    }
                }
            } else {
                format!("{}:{}", domain, port.unwrap())
            };

            use crate::util;
            let address = util::util::lookup_host(duration, &http_host).await?;

            let mut stream = socket_connect(
                typ,
                domain.to_string().into(),
                address,
                Some(domain.to_string()),
            )
            .await?;

            stream.set_stream_info(Some(
                self.stream_info.get().upstream_stream_flow_info.clone(),
            ));

            //___wait___
            //这样会失败
            // let stream =
            //     any_base::io::buf_stream::BufStream::from(any_base::io::buf_writer::BufWriter::new(
            //         any_base::io::buf_reader::BufReader::new(stream),
            //     ));

            let (stream, _) = match tokio::time::timeout(
                duration,
                tokio_tungstenite::client_async_with_config(http_req, stream, None),
            )
            .await
            {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };

            let stream: ArcMutexTokio<Box<dyn WebSocketStreamTrait>> =
                ArcMutexTokio::new(Box::new(stream));
            let session_id = {
                use crate::config::common_core;
                let common_core_conf = common_core::main_conf_mut(self.scc.ms()).await;
                let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
                session_id
            };

            self.wasm_stream_info
                .get_mut()
                .wasm_websocket_map
                .insert(session_id, stream);

            Ok(session_id)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_read(
        &mut self,
        fd: u64,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let ret: Result<Vec<u8>> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_websocket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            let msg = match tokio::time::timeout(duration, stream.next()).await {
                Ok(data) => data,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };

            let msg = match msg {
                Some(msg) => {
                    if msg.is_err() {
                        Vec::new()
                    } else {
                        let msg = msg?;
                        msg.into_data()
                    }
                }
                None => Vec::new(),
            };
            Ok(msg)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_write(
        &mut self,
        fd: u64,
        data: Vec<u8>,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_websocket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;

            match tokio::time::timeout(
                duration,
                stream.send(tokio_tungstenite::tungstenite::Message::Binary(data)),
            )
            .await
            {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };

            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_flush(
        &mut self,
        fd: u64,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_websocket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            match tokio::time::timeout(duration, stream.flush()).await {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };

            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_close(
        &mut self,
        fd: u64,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_websocket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, stream_info) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            match tokio::time::timeout(duration, stream.flush()).await {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            stream_info.get_mut().wasm_websocket_map.remove(&fd);
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }
}
