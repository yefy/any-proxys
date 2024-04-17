use crate::wasm::component::server::wasm_http;
use crate::wasm::component::server::wasm_socket;
use crate::wasm::component::server::wasm_websocket;
use crate::wasm::WasmHost;
use crate::wasm::{socket_connect, wasm_err};
use any_base::typ::ArcMutexTokio;
use async_trait::async_trait;
use std::sync::atomic::Ordering;

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

#[async_trait]
impl wasm_websocket::Host for WasmHost {
    async fn socket_connect(
        &mut self,
        typ: wasm_socket::SocketType,
        req: wasm_http::Request,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let http_req: http::Request<()> = match req.try_into() {
            Ok(r) => r,
            Err(_) => {
                return Ok(Ok(0));
            }
        };
        let domain = http_req.uri().host();
        if domain.is_none() {
            return Ok(Err("domain.is_none".to_string()));
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
        let address = util::util::lookup_host(tokio::time::Duration::from_secs(30), &http_host)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

        let mut stream = socket_connect(
            typ,
            domain.to_string().into(),
            address,
            Some(domain.to_string()),
        )
        .await
        .map_err(|e| wasm_err(e.to_string()))?;

        stream.set_stream_info(Some(
            self.stream_info.get().upstream_stream_flow_info.clone(),
        ));

        //___wait___
        //这样会失败
        // let stream =
        //     any_base::io::buf_stream::BufStream::from(any_base::io::buf_writer::BufWriter::new(
        //         any_base::io::buf_reader::BufReader::new(stream),
        //     ));

        let (stream, _) = tokio_tungstenite::client_async_with_config(http_req, stream, None)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

        let stream = ArcMutexTokio::new(stream);
        let session_id = {
            use crate::config::common_core;
            let common_core_conf = common_core::main_conf_mut(self.scc.ms()).await;
            let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
            session_id
        };

        self.stream_info
            .get_mut()
            .wasm_websocket_map
            .insert(session_id, stream);

        Ok(Ok(session_id))
    }

    async fn socket_read(
        &mut self,
        fd: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let stream = self
            .stream_info
            .get_mut()
            .wasm_websocket_map
            .get(&fd)
            .cloned();
        if stream.is_none() {
            let stream_info = self
                .stream_info
                .get_mut()
                .wasm_stream_info_map
                .get()
                .get(&fd)
                .cloned();
            if stream_info.is_none() {
                return Ok(Err("".to_string()));
            }
            let stream_info = stream_info.unwrap();
            let stream = stream_info.get().wasm_websocket_main.clone();
            let stream = &mut *stream.get_mut().await;
            let msg = stream.next().await;
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
            return Ok(Ok(msg));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        let msg = stream.next().await;
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
        Ok(Ok(msg))
    }

    async fn socket_write(
        &mut self,
        fd: u64,
        data: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self
            .stream_info
            .get_mut()
            .wasm_websocket_map
            .get(&fd)
            .cloned();
        if stream.is_none() {
            let stream_info = self
                .stream_info
                .get_mut()
                .wasm_stream_info_map
                .get()
                .get(&fd)
                .cloned();
            if stream_info.is_none() {
                return Ok(Err("".to_string()));
            }
            let stream_info = stream_info.unwrap();
            let stream = stream_info.get().wasm_websocket_main.clone();
            let stream = &mut *stream.get_mut().await;
            let ret = stream
                .send(tokio_tungstenite::tungstenite::Message::Binary(data))
                .await;
            if let Err(e) = ret {
                return Ok(Err(format!("{:?}", e)));
            }
            return Ok(Ok(()));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        let ret = stream
            .send(tokio_tungstenite::tungstenite::Message::Binary(data))
            .await;
        if let Err(e) = ret {
            return Ok(Err(format!("{:?}", e)));
        }
        Ok(Ok(()))
    }

    async fn socket_flush(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self
            .stream_info
            .get_mut()
            .wasm_websocket_map
            .get(&fd)
            .cloned();
        if stream.is_none() {
            let stream_info = self
                .stream_info
                .get_mut()
                .wasm_stream_info_map
                .get()
                .get(&fd)
                .cloned();
            if stream_info.is_none() {
                return Ok(Err("".to_string()));
            }
            let stream_info = stream_info.unwrap();
            let stream = stream_info.get().wasm_websocket_main.clone();
            let stream = &mut *stream.get_mut().await;
            stream.flush().await?;
            return Ok(Ok(()));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        stream.flush().await?;

        Ok(Ok(()))
    }

    async fn socket_close(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self
            .stream_info
            .get_mut()
            .wasm_websocket_map
            .get(&fd)
            .cloned();
        if stream.is_none() {
            let stream_info = self
                .stream_info
                .get_mut()
                .wasm_stream_info_map
                .get()
                .get(&fd)
                .cloned();
            if stream_info.is_none() {
                return Ok(Err("".to_string()));
            }
            let stream_info = stream_info.unwrap();
            let stream = stream_info.get().wasm_websocket_main.clone();
            {
                let stream = &mut *stream.get_mut().await;
                stream.flush().await?;
            }
            stream.set_nil().await;
            return Ok(Ok(()));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        stream.flush().await?;
        self.stream_info.get_mut().wasm_websocket_map.remove(&fd);

        Ok(Ok(()))
    }
}
