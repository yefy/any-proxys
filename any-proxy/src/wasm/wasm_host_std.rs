use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::util::http_serverless;
use crate::util::default_config;
use crate::wasm::component::server::wasm_http;
use crate::wasm::component::server::wasm_std;
use crate::wasm::{get_timer_timeout, session_send, WasmHost, WasmStreamInfo};
use any_base::typ::Share;
use anyhow::Result;
use async_trait::async_trait;
use http::Response;
use hyper::body::HttpBody;
use hyper::http::{HeaderName, HeaderValue};
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;

#[async_trait]
impl wasm_std::Host for WasmHost {
    async fn anyproxy_version(&mut self) -> wasmtime::Result<std::result::Result<String, String>> {
        Ok(Ok(default_config::HTTP_VERSION.to_string()))
    }

    async fn variable(
        &mut self,
        name: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        use crate::proxy::stream_var;
        let var = stream_var::find(&name, &*self.stream_info.get());
        if var.is_err() {
            return Ok(Ok(None));
        }
        let var = var.unwrap();
        if var.is_none() {
            return Ok(Ok(None));
        }
        let var = var.unwrap().to_string();
        if var.is_err() {
            return Ok(Ok(None));
        }

        Ok(Ok(Some(var.unwrap())))
    }

    async fn sleep(&mut self, time_ms: u64) -> wasmtime::Result<()> {
        tokio::time::sleep(tokio::time::Duration::from_millis(time_ms)).await;
        Ok(())
    }

    async fn curr_session_id(&mut self) -> wasmtime::Result<u64> {
        let session_id = self.session_id;
        Ok(session_id)
    }

    async fn main_session_id(&mut self) -> wasmtime::Result<u64> {
        let session_id = self.stream_info.get().session_id;
        Ok(session_id)
    }

    async fn main_fd(&mut self) -> wasmtime::Result<u64> {
        let session_id = self.stream_info.get().session_id;
        Ok(session_id)
    }

    async fn new_session_id(&mut self) -> wasmtime::Result<u64> {
        use crate::config::common_core::get_session_id;
        let session_id = get_session_id();
        Ok(session_id)
    }

    async fn session_send(
        &mut self,
        session_id: u64,
        cmd: i64,
        value: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        session_send(&self.wasm_stream_info_map, session_id, cmd, value).await
    }

    async fn session_request(
        &mut self,
        session_id: u64,
        cmd: i64,
        value: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<Option<Vec<u8>>, String>> {
        let ret: Result<Option<Vec<u8>>> = async {
            let stream_info = self.wasm_stream_info_map.get().get(&session_id).cloned();
            if stream_info.is_none() {
                return Err(anyhow::anyhow!("stream_info.is_none"));
            }
            let stream_info = stream_info.unwrap();
            let wasm_session_sender = stream_info.get().wasm_session_sender.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            wasm_session_sender.send((cmd, value, Some(tx))).await?;
            let data = rx.await?;
            Ok(data)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn session_response(
        &mut self,
        fd: u64,
        value: Option<Vec<u8>>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            let wasm_session_response = self
                .wasm_stream_info
                .get_mut()
                .wasm_session_response_map
                .remove(&fd);
            if wasm_session_response.is_none() {
                return Err(anyhow::anyhow!("wasm_session_response.is_none"));
            }
            let wasm_session_response = wasm_session_response.unwrap();
            wasm_session_response
                .send(value)
                .map_err(|_e| anyhow::anyhow!("err:send"))?;
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn session_recv(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<(u64, i64, Vec<u8>), String>> {
        let ret: Result<(u64, i64, Vec<u8>)> = async {
            let wasm_session_receiver = self.wasm_stream_info.get().wasm_session_receiver.clone();
            let (cmd, data, tx) = wasm_session_receiver.recv().await?;
            let fd = if tx.is_some() {
                let fd = self
                    .new_session_id()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                self.wasm_stream_info
                    .get_mut()
                    .wasm_session_response_map
                    .insert(fd, tx.unwrap());
                fd
            } else {
                0
            };
            Ok((fd, cmd, data))
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn session_try_recv(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<Option<(u64, i64, Vec<u8>)>, String>> {
        let ret: Result<Option<(u64, i64, Vec<u8>)>> = async {
            let wasm_session_receiver = self.wasm_stream_info.get().wasm_session_receiver.clone();
            let ret = wasm_session_receiver.try_recv();
            if let Err(e) = ret {
                if let async_channel::TryRecvError::Empty = e {
                    return Ok(None);
                }
                return Err(e)?;
            }

            let (cmd, data, tx) = ret.unwrap();
            let fd = if tx.is_some() {
                let fd = self
                    .new_session_id()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                self.wasm_stream_info
                    .get_mut()
                    .wasm_session_response_map
                    .insert(fd, tx.unwrap());
                fd
            } else {
                0
            };
            Ok(Some((fd, cmd, data)))
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn add_timer(&mut self, time_ms: u64, cmd: i64, value: Vec<u8>) -> wasmtime::Result<()> {
        let stream_info = &mut *self.wasm_stream_info.get_mut();
        stream_info.wasm_timers.insert(cmd, (time_ms as i64, value));
        Ok(())
    }

    async fn del_timer(&mut self, cmd: i64) -> wasmtime::Result<()> {
        let stream_info = &mut *self.wasm_stream_info.get_mut();
        stream_info.wasm_timers.remove(&cmd);
        Ok(())
    }

    async fn get_timer_timeout(&mut self, time_ms: u64) -> wasmtime::Result<Vec<(i64, Vec<u8>)>> {
        get_timer_timeout(time_ms, &self.wasm_stream_info).await
    }

    async fn in_add_headers(
        &mut self,
        headers: Vec<(String, String)>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            for (k, v) in headers {
                rctx.r_in.upstream_headers.insert(
                    HeaderName::from_str(k.as_str())?,
                    HeaderValue::from_str(&v)?,
                );
            }
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_add_header(
        &mut self,
        k: String,
        v: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            rctx.r_in.upstream_headers.insert(
                HeaderName::from_str(k.as_str())?,
                HeaderValue::from_str(&v)?,
            );
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_del_headers(
        &mut self,
        headers: Vec<String>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            for k in headers {
                rctx.r_in.upstream_headers.remove(k.as_str());
            }
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_del_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            rctx.r_in.upstream_headers.remove(k.as_str());
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_is_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<bool, String>> {
        let ret: Result<bool> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            let is_ok = rctx.r_in.upstream_headers.get(k.as_str()).is_some();
            Ok(is_ok)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_get_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let ret: Result<Option<String>> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            let v = rctx.r_in.upstream_headers.get(k.as_str());
            if v.is_none() {
                return Ok(None);
            }
            let v = v.unwrap();
            let v = v.to_str()?;
            Ok(Some(v.to_string()))
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_get_request(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<wasm_std::Request, String>> {
        let ret: Result<wasm_std::Request> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let wasm_req = in_to_wasm_request(&self.r).await?;
            Ok(wasm_req)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn in_body_read_exact(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let ret: Result<Vec<u8>> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let body = {
                let rctx = &mut *self.r.ctx.get_mut();
                let body = rctx.r_in.body.take();
                body
            };
            if body.is_none() {
                return Ok(Vec::new());
            }
            let mut body = body.unwrap();

            let mut data = Vec::with_capacity(1024);
            loop {
                let body = body.data().await;
                if body.is_none() {
                    break;
                }
                let body = body.unwrap();
                if body.is_err() {
                    break;
                }
                let body = body.unwrap();
                data.extend_from_slice(body.to_bytes().unwrap().as_ref());
            }
            Ok(data)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_add_headers(
        &mut self,
        headers: Vec<(String, String)>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            for (k, v) in headers {
                rctx.r_out.headers.insert(
                    HeaderName::from_str(k.as_str())?,
                    HeaderValue::from_str(&v)?,
                );
            }
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_add_header(
        &mut self,
        k: String,
        v: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            rctx.r_out.headers.insert(
                HeaderName::from_str(k.as_str())?,
                HeaderValue::from_str(&v)?,
            );
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_del_headers(
        &mut self,
        headers: Vec<String>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            for k in headers {
                rctx.r_out.headers.remove(k.as_str());
            }
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_del_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            rctx.r_out.headers.remove(k.as_str());
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_is_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<bool, String>> {
        let ret: Result<bool> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            let is_ok = rctx.r_out.headers.get(k.as_str()).is_some();
            Ok(is_ok)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_get_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let ret: Result<Option<String>> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            let rctx = &mut *self.r.ctx.get_mut();
            let v = rctx.r_out.headers.get(k.as_str());

            if v.is_none() {
                return Ok(None);
            }
            let v = v.unwrap();
            let v = v.to_str()?;
            Ok(Some(v.to_string()))
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_set_response(
        &mut self,
        wasm_res: wasm_std::Response,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            wasn_response_to_out(&self.r, wasm_res)?;
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn out_response(
        &mut self,
        wasm_res: wasm_std::Response,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ret: Result<()> = async {
            if self.r.is_none() {
                return Err(anyhow::anyhow!("self.r.is_none"));
            }
            if self.scc.is_none() {
                return Err(anyhow::anyhow!("scc.is_none"));
            }
            wasn_response_to_out(&self.r, wasm_res)?;
            http_serverless(&self.r).await?;
            Ok(())
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }

    async fn spawn(
        &mut self,
        tye: Option<i64>,
        config: Option<String>,
    ) -> wasmtime::Result<std::result::Result<u64, String>> {
        let ret: Result<u64> = async {
            let wasm_spawn_sender = self.stream_info.get().wasm_spawn_sender.clone();
            use crate::config::common_core::get_session_id;
            let session_id = get_session_id();

            let stream_info = self.stream_info.clone();
            let wasm_plugin = self.wasm_plugin.clone();
            let wasm_stream_info_map = self.wasm_stream_info_map.clone();
            let wasm_stream_info = WasmStreamInfo::new();
            let wasm_stream_info = Share::new(wasm_stream_info);
            wasm_stream_info_map
                .get_mut()
                .insert(session_id, wasm_stream_info.clone());
            let wasm_host = WasmHost::new(
                session_id,
                stream_info,
                wasm_plugin,
                wasm_stream_info.clone(),
            );
            wasm_spawn_sender.send((tye, config, wasm_host)).await?;
            Ok(session_id)
        }
        .await;
        Ok(ret.map_err(|e| e.to_string()))
    }
}

pub async fn in_to_wasm_request(http_r: &Arc<HttpStreamRequest>) -> Result<wasm_http::Request> {
    let (method, uri, headers, body) = {
        let r_in = &mut http_r.ctx.get_mut().r_in;
        let method = match r_in.method {
            hyper::Method::GET => wasm_http::Method::Get,
            hyper::Method::POST => wasm_http::Method::Post,
            hyper::Method::PUT => wasm_http::Method::Put,
            hyper::Method::DELETE => wasm_http::Method::Delete,
            hyper::Method::HEAD => wasm_http::Method::Head,
            hyper::Method::OPTIONS => wasm_http::Method::Options,
            hyper::Method::PATCH => wasm_http::Method::Patch,
            _ => {
                return Err(anyhow::anyhow!("r_in.method"));
            }
        };

        let uri = r_in.uri.to_string();

        let mut headers: Vec<(String, String)> = vec![];
        for (key, value) in &r_in.headers {
            headers.push((key.to_string(), value.to_str()?.to_string()));
        }
        let body = r_in.body.take();
        (method, uri, headers, body)
    };

    let body = if body.is_none() {
        None
    } else {
        let mut body = body.unwrap();

        let mut data = Vec::with_capacity(1024);
        loop {
            let body = body.data().await;
            if body.is_none() {
                break;
            }
            let body = body.unwrap();
            if body.is_err() {
                break;
            }
            let body = body.unwrap();
            data.extend_from_slice(body.to_bytes().unwrap().as_ref());
        }
        Some(data)
    };

    let params = Vec::new();

    Ok(wasm_http::Request {
        method,
        uri,
        headers,
        params,
        body,
    })
}

pub fn wasn_response_to_out(
    http_r: &Arc<HttpStreamRequest>,
    wasm_res: wasm_http::Response,
) -> Result<()> {
    let http_r_ctx = &mut http_r.ctx.get_mut();
    let wasm_http::Response {
        status,
        headers,
        body,
    } = wasm_res;

    let (body, len) = if body.is_some() {
        let body = body.unwrap();
        let len = body.len();
        (hyper::Body::from(body), len)
    } else {
        (hyper::Body::default(), 0)
    };

    let mut response = Response::builder()
        .status(hyper::StatusCode::from_u16(status)?)
        .body(body)?;

    if headers.is_some() {
        let headers = headers.unwrap();
        for (key, value) in headers {
            response.headers_mut().insert(
                http::HeaderName::try_from(key)?,
                http::HeaderValue::try_from(value)?,
            );
        }
    }

    if len > 0 {
        response
            .headers_mut()
            .insert(http::header::CONTENT_LENGTH, HeaderValue::from(len));
    }

    http_r_ctx.wasm_response = Some(response);

    Ok(())
}
