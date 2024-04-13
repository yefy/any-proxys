use crate::wasm::component::server::wasm_tcp;
use crate::wasm::wasm_err;
use crate::wasm::WasmHost;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
impl wasm_tcp::Host for WasmHost {
    async fn socket_connect(
        &mut self,
        typ: wasm_tcp::SocketType,
        addr: String,
        ssl_domain: Option<String>,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        use crate::util;
        let address = util::util::lookup_host(tokio::time::Duration::from_secs(30), &addr)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;

        let connect = match typ {
            wasm_tcp::SocketType::Tcp => {
                use crate::config::config_toml::default_tcp_config;
                use crate::stream::connect;
                use crate::tcp::connect::Connect;
                let tcp_config = default_tcp_config().pop().unwrap();
                let connect = Box::new(Connect::new(addr.into(), address, Arc::new(tcp_config)));
                let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
                connect
            }
            wasm_tcp::SocketType::Ssl => {
                use crate::config::config_toml::default_tcp_config;
                use crate::ssl::connect::Connect;
                use crate::stream::connect;
                if ssl_domain.is_none() {
                    return Ok(Err("ssl_domain.is_none".to_string()));
                }
                let tcp_config = default_tcp_config().pop().unwrap();
                let connect = Box::new(Connect::new(
                    addr.into(),
                    address,
                    ssl_domain.unwrap(),
                    Arc::new(tcp_config),
                ));
                let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
                connect
            }
            wasm_tcp::SocketType::Quic => {
                use crate::config::config_toml::default_quic_config;
                use crate::quic::connect::Connect;
                use crate::quic::endpoints;
                use crate::stream::connect;
                if ssl_domain.is_none() {
                    return Ok(Err("ssl_domain.is_none".to_string()));
                }
                //let address: SocketAddr = SocketAddr::from_str(&addr)?;
                let quic_config = default_quic_config().pop().unwrap();
                let endpoints = endpoints::Endpoints::new(&quic_config, false)
                    .map_err(|e| wasm_err(e.to_string()))?;
                let endpoints = Arc::new(endpoints);

                let connect = Box::new(Connect::new(
                    addr.into(),
                    address,
                    ssl_domain.unwrap(),
                    endpoints,
                    Arc::new(quic_config),
                ));
                let connect: Arc<Box<dyn connect::Connect>> = Arc::new(connect);
                connect
            }
        };
        use any_base::typ::ArcMutexTokio;
        use core::sync::atomic::Ordering;
        let (mut stream, _) = connect
            .connect(None, None, None)
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
    ) -> wasmtime::Result<std::result::Result<String, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        if stream.is_none() {
            return Ok(Err("".to_string()));
        }
        let stream = stream.unwrap();
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
            return Ok(Ok("".to_string()));
        }
        let size = size.unwrap();

        unsafe { data.set_len(size) };

        Ok(Ok(unsafe { String::from_utf8_unchecked(data) }))
    }

    async fn socket_read_exact(
        &mut self,
        fd: u64,
        size: u64,
    ) -> wasmtime::Result<std::result::Result<String, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        if stream.is_none() {
            return Ok(Err("".to_string()));
        }
        let stream = stream.unwrap();
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
            return Ok(Ok("".to_string()));
        }
        let size = size.unwrap();

        unsafe { data.set_len(size) };

        Ok(Ok(unsafe { String::from_utf8_unchecked(data) }))
    }

    async fn socket_write(
        &mut self,
        fd: u64,
        data: String,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        if stream.is_none() {
            return Ok(Err("".to_string()));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        let size = stream.write(data.as_bytes()).await;
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
        data: String,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        if stream.is_none() {
            return Ok(Err("".to_string()));
        }
        let size = data.len();
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        let ret = stream.write_all(data.as_bytes()).await;
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

        Ok(Ok(size as u64))
    }

    async fn socket_flush(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream = self.stream_info.get_mut().wasm_socket_map.get(&fd).cloned();
        if stream.is_none() {
            return Ok(Err("".to_string()));
        }
        let stream = stream.unwrap();
        let stream = &mut *stream.get_mut().await;
        use tokio::io::AsyncWriteExt;
        stream.flush().await?;

        Ok(Ok(()))
    }

    async fn socket_close(&mut self, fd: u64) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret = self.socket_flush(fd).await?;
        if ret.is_err() {
            return Ok(ret);
        }
        self.stream_info.get_mut().wasm_socket_map.remove(&fd);
        Ok(Ok(()))
    }
}
