use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::util::default_config;
use crate::wasm::component::server::wasm_http;
use crate::wasm::component::server::wasm_std;
use crate::wasm::{wasm_err, WasmHost};
use anyhow::Result;
use async_trait::async_trait;
use hyper::body::HttpBody;
use hyper::http::{HeaderName, HeaderValue};
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::atomic::Ordering;
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
        let session_id = self.stream_info.get().session_id;
        Ok(session_id)
    }

    async fn curr_fd(&mut self) -> wasmtime::Result<u64> {
        let session_id = self.stream_info.get().session_id;
        Ok(session_id)
    }

    async fn new_session_id(&mut self) -> wasmtime::Result<u64> {
        if self.scc.is_none() {
            return Ok(0);
        }
        let session_id = {
            use crate::config::common_core;
            let common_core_conf = common_core::main_conf_mut(self.scc.ms()).await;
            let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
            session_id
        };
        Ok(session_id)
    }

    async fn session_send(
        &mut self,
        session_id: u64,
        value: Vec<u8>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let stream_info = self
            .stream_info
            .get_mut()
            .wasm_stream_info_map
            .get()
            .get(&session_id)
            .cloned();
        if stream_info.is_none() {
            return Ok(Err("".to_string()));
        }
        let stream_info = stream_info.unwrap();
        let wasm_session_sender = stream_info.get().wasm_session_sender.clone();
        let ret = wasm_session_sender
            .send(value)
            .await
            .map_err(|e| e.to_string());
        if let Err(e) = ret {
            return Ok(Err(e));
        }
        Ok(Ok(()))
    }

    async fn session_recv(&mut self) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let wasm_session_receiver = self.stream_info.get().wasm_session_receiver.clone();
        let ret = wasm_session_receiver.recv().await;
        if let Err(e) = &ret {
            return Ok(Err(e.to_string()));
        }
        Ok(Ok(ret.unwrap()))
    }

    async fn add_timer(&mut self, time_ms: u64, key: u64, value: Vec<u8>) -> wasmtime::Result<()> {
        let stream_info = &mut *self.stream_info.get_mut();
        stream_info.wasm_timers.insert(key, (time_ms as i64, value));
        Ok(())
    }

    async fn del_timer(&mut self, key: u64) -> wasmtime::Result<()> {
        let stream_info = &mut *self.stream_info.get_mut();
        stream_info.wasm_timers.remove(&key);
        Ok(())
    }

    async fn get_timer_timeout(&mut self, time_ms: u64) -> wasmtime::Result<Vec<Vec<u8>>> {
        let stream_info = &mut *self.stream_info.get_mut();
        let mut expire_keys = Vec::with_capacity(10);
        for (key, (_time_ms, _)) in &mut stream_info.wasm_timers.iter_mut() {
            *_time_ms -= time_ms as i64;
            if *_time_ms <= 0 {
                expire_keys.push(*key);
            }
        }

        let mut values = Vec::with_capacity(10);
        for key in expire_keys {
            let timer = stream_info.wasm_timers.remove(&key);
            if timer.is_none() {
                continue;
            }
            let (_, value) = timer.unwrap();
            values.push(value);
        }

        Ok(values)
    }

    async fn in_add_headers(
        &mut self,
        headers: Vec<(String, String)>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        for (k, v) in headers {
            r_ctx.r_in.headers_upstream.insert(
                HeaderName::from_str(k.as_str())?,
                HeaderValue::from_str(&v)?,
            );
        }
        Ok(Ok(()))
    }

    async fn in_add_header(
        &mut self,
        k: String,
        v: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        r_ctx.r_in.headers_upstream.insert(
            HeaderName::from_str(k.as_str())?,
            HeaderValue::from_str(&v)?,
        );
        Ok(Ok(()))
    }

    async fn in_del_headers(
        &mut self,
        headers: Vec<String>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        for k in headers {
            r_ctx.r_in.headers_upstream.remove(k.as_str());
        }
        Ok(Ok(()))
    }

    async fn in_del_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        r_ctx.r_in.headers_upstream.remove(k.as_str());
        Ok(Ok(()))
    }

    async fn in_is_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<bool, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        let is_ok = r_ctx.r_in.headers_upstream.get(k.as_str()).is_some();
        Ok(Ok(is_ok))
    }

    async fn in_get_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        let v = r_ctx.r_in.headers_upstream.get(k.as_str());
        if v.is_none() {
            return Ok(Ok(None));
        }
        let v = v.unwrap();
        let v = v.to_str()?;
        Ok(Ok(Some(v.to_string())))
    }

    async fn in_get_request(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<wasm_std::Request, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r = r.unwrap();
        let wasm_req = in_to_wasm_request(r)
            .await
            .map_err(|e| wasm_err(e.to_string()))?;
        Ok(Ok(wasm_req))
    }

    async fn in_body_read_exact(
        &mut self,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let body = {
            let r_ctx = &mut *r.ctx.get_mut();
            let body = r_ctx.r_in.body.take();
            body
        };
        if body.is_none() {
            return Ok(Ok(Vec::new()));
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
        Ok(Ok(data))
    }

    async fn out_add_headers(
        &mut self,
        headers: Vec<(String, String)>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        for (k, v) in headers {
            r_ctx.r_out.headers.insert(
                HeaderName::from_str(k.as_str())?,
                HeaderValue::from_str(&v)?,
            );
        }
        Ok(Ok(()))
    }

    async fn out_add_header(
        &mut self,
        k: String,
        v: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        r_ctx.r_out.headers.insert(
            HeaderName::from_str(k.as_str())?,
            HeaderValue::from_str(&v)?,
        );
        Ok(Ok(()))
    }

    async fn out_del_headers(
        &mut self,
        headers: Vec<String>,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        for k in headers {
            r_ctx.r_out.headers.remove(k.as_str());
        }
        Ok(Ok(()))
    }

    async fn out_del_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        r_ctx.r_out.headers.remove(k.as_str());
        Ok(Ok(()))
    }

    async fn out_is_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<bool, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        let is_ok = r_ctx.r_out.headers.get(k.as_str()).is_some();
        Ok(Ok(is_ok))
    }

    async fn out_get_header(
        &mut self,
        k: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r_ctx = &mut *r.ctx.get_mut();
        let v = r_ctx.r_out.headers.get(k.as_str());

        if v.is_none() {
            return Ok(Ok(None));
        }
        let v = v.unwrap();
        let v = v.to_str()?;
        Ok(Ok(Some(v.to_string())))
    }

    async fn out_set_response(
        &mut self,
        wasm_res: wasm_std::Response,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let r = self.stream_info.get().http_r.clone();
        if r.is_none() {
            return Ok(Err("r.is_none()".to_string()));
        }
        let r = r.unwrap();
        wasn_response_to_out(r, wasm_res).map_err(|e| wasm_err(e.to_string()))?;
        Ok(Ok(()))
    }
}

pub async fn in_to_wasm_request(http_r: Arc<HttpStreamRequest>) -> Result<wasm_http::Request> {
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
    http_r: Arc<HttpStreamRequest>,
    wasm_res: wasm_http::Response,
) -> Result<()> {
    let http_r_ctx = &mut http_r.ctx.get_mut();
    let r_out = &mut http_r_ctx.r_out;
    let wasm_http::Response {
        status,
        headers,
        body,
    } = wasm_res;
    r_out.status = hyper::StatusCode::from_u16(status)?;
    if headers.is_some() {
        let headers = headers.unwrap();
        for (key, value) in headers {
            r_out.headers.insert(
                http::HeaderName::try_from(key)?,
                http::HeaderValue::try_from(value)?,
            );
        }
    }
    if body.is_some() {
        let body = body.unwrap();
        http_r_ctx.wasm_body = Some(hyper::Body::from(body));
    }
    Ok(())
}
