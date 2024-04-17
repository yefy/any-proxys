use crate::wasm::component::server::wasm_socket;
use crate::wasm::WasmHost;
use crate::wasm::{socket_connect, wasm_err};
use any_base::typ::ArcMutexTokio;
use async_trait::async_trait;
use std::sync::atomic::Ordering;

#[async_trait]
impl wasm_socket::Host for WasmHost {
    async fn socket_connect(
        &mut self,
        typ: wasm_socket::SocketType,
        addr: String,
        ssl_domain: Option<String>,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        use crate::util;
        let address = util::util::lookup_host(tokio::time::Duration::from_secs(30), &addr)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

        let mut stream = socket_connect(typ, addr.into(), address, ssl_domain)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

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

        self.stream_info
            .get_mut()
            .wasm_socket_map
            .insert(session_id, stream);

        Ok(Ok(session_id))
    }

    async fn socket_read(
        &mut self,
        fd: u64,
        size: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let stream = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            stream.unwrap()
        } else {
            stream.unwrap()
        };

        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncReadExt;
        let mut data = vec![0; size as usize];
        let size = stream.read(data.as_mut_slice()).await;
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
            return Ok(Ok(Vec::new()));
        }
        let size = size.unwrap();

        unsafe { data.set_len(size) };

        Ok(Ok(data))
    }

    async fn socket_read_exact(
        &mut self,
        fd: u64,
        size: u64,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let stream = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            stream.unwrap()
        } else {
            stream.unwrap()
        };
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncReadExt;
        let mut data = vec![0; size as usize];
        let size = stream.read_exact(data.as_mut_slice()).await;
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
            return Ok(Ok(Vec::new()));
        }
        let size = size.unwrap();

        unsafe { data.set_len(size) };

        Ok(Ok(data))
    }

    async fn socket_write(
        &mut self,
        fd: u64,
        data: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let stream = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            stream.unwrap()
        } else {
            stream.unwrap()
        };
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        let size = stream.write(data.as_slice()).await;
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
            return Ok(Ok(0));
        }
        let size = size.unwrap();

        Ok(Ok(size as u64))
    }

    async fn socket_write_all(
        &mut self,
        fd: u64,
        data: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let stream = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            stream.unwrap()
        } else {
            stream.unwrap()
        };
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        let ret = stream.write_all(data.as_slice()).await;
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
            return Ok(Ok(0));
        }

        Ok(Ok(data.len() as u64))
    }

    async fn socket_flush(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let stream = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            stream.unwrap()
        } else {
            stream.unwrap()
        };
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        stream.flush().await?;

        Ok(Ok(()))
    }

    async fn socket_close(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        let (stream, stream_info) = if stream.is_none() {
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

            let stream = stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
            if stream.is_none() {
                return Ok(Err("".to_string()));
            }
            (stream.unwrap(), stream_info.clone())
        } else {
            (stream.unwrap(), self.stream_info.clone())
        };
        {
            let stream = &mut *stream.get_mut().await;
            use tokio::io::AsyncWriteExt;
            stream.flush().await?;
        }

        stream_info.get_mut().wasm_socket_map.remove(&fd);
        Ok(Ok(()))
    }
}
