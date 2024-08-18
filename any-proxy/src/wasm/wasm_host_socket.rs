use crate::wasm::component::server::wasm_socket;
use crate::wasm::WasmHost;
use crate::wasm::{socket_connect, WasmStreamInfo};
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutexTokio, Share};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::Ordering;

impl WasmHost {
    pub fn get_socket_stream(
        &self,
        fd: &u64,
    ) -> Option<(
        ArcMutexTokio<any_base::io::buf_stream::BufStream<StreamFlow>>,
        Share<WasmStreamInfo>,
    )> {
        let stream = self
            .wasm_stream_info
            .get_mut()
            .wasm_socket_map
            .get(fd)
            .cloned();
        let stream = if stream.is_none() {
            let wasm_stream_info = self.wasm_stream_info_map.get().get(&fd).cloned();
            if wasm_stream_info.is_none() {
                return None;
            }
            let wasm_stream_info = wasm_stream_info.unwrap();

            let stream = wasm_stream_info.get_mut().wasm_socket_map.get(fd).cloned();
            if stream.is_none() {
                return None;
            }
            Some((stream.unwrap(), wasm_stream_info.clone()))
        } else {
            Some((stream.unwrap(), self.wasm_stream_info.clone()))
        };
        stream
    }
}

#[async_trait]
impl wasm_socket::Host for WasmHost {
    async fn socket_connect(
        &mut self,
        typ: wasm_socket::SocketType,
        addr: String,
        ssl_domain: Option<String>,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let ret: Result<u64> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            use crate::util;
            let address = util::util::lookup_host(duration, &addr).await?;

            let mut stream = match tokio::time::timeout(
                duration,
                socket_connect(typ, addr.into(), address, ssl_domain),
            )
            .await
            {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };

            stream.set_stream_info(Some(
                self.stream_info.get().upstream_stream_flow_info.clone(),
            ));
            let stream = any_base::io::buf_stream::BufStream::new(stream);
            let stream = ArcMutexTokio::new(stream);
            let session_id = {
                use crate::config::common_core;
                let common_core_conf = common_core::main_conf_mut(self.scc.ms()).await;
                let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
                session_id
            };

            self.wasm_stream_info
                .get_mut()
                .wasm_socket_map
                .insert(session_id, stream);

            Ok(session_id)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_read(
        &mut self,
        fd: u64,
        size: u64,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let ret: Result<Vec<u8>> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncReadExt;
            let mut data = vec![0; size as usize];
            let size = match tokio::time::timeout(duration, stream.read(data.as_mut_slice())).await
            {
                Ok(data) => data,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            if size.is_err() {
                let is_close = self
                    .stream_info
                    .get()
                    .upstream_stream_flow_info
                    .get()
                    .is_close();
                if !is_close {
                    size?;
                }
                return Ok(Vec::new());
            }
            let size = size.unwrap();

            unsafe { data.set_len(size) };

            Ok(data)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_read_exact(
        &mut self,
        fd: u64,
        size: u64,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let ret: Result<Vec<u8>> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncReadExt;
            let mut data = vec![0; size as usize];
            let size = match tokio::time::timeout(duration, stream.read_exact(data.as_mut_slice()))
                .await
            {
                Ok(data) => data,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            if size.is_err() {
                let is_close = self
                    .stream_info
                    .get()
                    .upstream_stream_flow_info
                    .get()
                    .is_close();
                if !is_close {
                    size?;
                }
                return Ok(Vec::new());
            }
            let size = size.unwrap();

            unsafe { data.set_len(size) };

            Ok(data)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_write(
        &mut self,
        fd: u64,
        data: Vec<u8>,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let ret: Result<u64> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncWriteExt;
            let size = match tokio::time::timeout(duration, stream.write(data.as_slice())).await {
                Ok(data) => data,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            if size.is_err() {
                let is_close = self
                    .stream_info
                    .get()
                    .upstream_stream_flow_info
                    .get()
                    .is_close();
                if !is_close {
                    size?;
                }
                return Ok(0);
            }
            let size = size.unwrap();

            Ok(size as u64)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn socket_write_all(
        &mut self,
        fd: u64,
        data: Vec<u8>,
        timeout_ms: u64,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let ret: Result<u64> = async {
            let duration = tokio::time::Duration::from_millis(timeout_ms);
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncWriteExt;
            let ret = match tokio::time::timeout(duration, stream.write_all(data.as_slice())).await
            {
                Ok(data) => data,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            if ret.is_err() {
                let is_close = self
                    .stream_info
                    .get()
                    .upstream_stream_flow_info
                    .get()
                    .is_close();
                if !is_close {
                    ret?;
                }
                return Ok(0);
            }

            Ok(data.len() as u64)
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
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, _) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncWriteExt;
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
            let stream = self.get_socket_stream(&fd);
            if stream.is_none() {
                return Err(anyhow::anyhow!("stream.is_none"));
            }
            let (stream, wasm_stream_info) = stream.unwrap();
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncWriteExt;
            match tokio::time::timeout(duration, stream.flush()).await {
                Ok(data) => data?,
                Err(_e) => return Err(anyhow::anyhow!("timeout")),
            };
            wasm_stream_info.get_mut().wasm_socket_map.remove(&fd);

            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }
}
