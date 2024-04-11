use crate::config::net_core_wasm::WasmHashValue;
use crate::proxy::http_proxy::http_header_parse::content_length;
use crate::proxy::http_proxy::HttpStreamRequest;
use crate::proxy::StreamInfo;
use crate::util::default_config;
use any_base::typ::ArcRwLock;
use any_base::typ::Share;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use hyper::body::HttpBody;
use hyper::http::{HeaderName, HeaderValue};
use std::sync::Arc;
use wasmtime::component::ResourceTable;
use wasmtime::component::*;
use wasmtime_wasi::preview2::{command, WasiCtx, WasiCtxBuilder, WasiView};

use std::convert::TryFrom;
use std::convert::TryInto;
use std::str::FromStr;

use crate::wasm_bindings_name;
use crate::wasm_bindings_name_t;

#[derive(Clone)]
pub struct WasmHost {
    stream_info: Share<StreamInfo>,
}

impl WasmHost {
    pub fn new(stream_info: Share<StreamInfo>) -> Self {
        Self { stream_info }
    }
}

#[async_trait]
impl component::server::wasm_std::Host for WasmHost {
    async fn anyproxy_version(&mut self) -> wasmtime::Result<std::result::Result<String, String>> {
        Ok(Ok(default_config::HTTP_VERSION.to_string()))
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

    async fn log_enabled(&mut self, level: wasm_bindings_name!(Level)) -> wasmtime::Result<bool> {
        let level = match level {
            <wasm_bindings_name!(Level)>::Error => log::Level::Error,
            <wasm_bindings_name!(Level)>::Warn => log::Level::Warn,
            <wasm_bindings_name!(Level)>::Info => log::Level::Info,
            <wasm_bindings_name!(Level)>::Debug => log::Level::Debug,
            <wasm_bindings_name!(Level)>::Trace => log::Level::Trace,
        };
        Ok(log::log_enabled!(level))
    }

    async fn log_error(
        &mut self,
        str: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        log::error!(target: "wasm", "{}", str);
        Ok(Ok(()))
    }

    async fn log_warn(&mut self, str: String) -> wasmtime::Result<std::result::Result<(), String>> {
        log::warn!(target: "wasm", "{}", str);
        Ok(Ok(()))
    }
    async fn log_info(&mut self, str: String) -> wasmtime::Result<std::result::Result<(), String>> {
        log::info!(target: "wasm", "{}", str);
        Ok(Ok(()))
    }
    async fn log_debug(
        &mut self,
        str: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        log::debug!(target: "wasm", "{}", str);
        Ok(Ok(()))
    }
    async fn log_trace(
        &mut self,
        str: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        log::trace!(target: "wasm", "{}", str);
        Ok(Ok(()))
    }

    async fn handle_http(
        &mut self,
        req: wasm_bindings_name!(Request),
    ) -> wasmtime::Result<std::result::Result<wasm_bindings_name!(Response), String>> {
        let http_req: http::Request<hyper::Body> = match req.try_into() {
            Ok(r) => r,
            Err(_) => {
                return Ok(Ok(wasm_bindings_name_t!(Response {
                    status: 500,
                    headers: None,
                    body: None,
                })));
            }
        };

        let client = hyper::Client::new();
        let http_resp = client.request(http_req, None).await?;
        let wasn_resp = http_response_to_wasn_response(http_resp).await;
        Ok(Ok(match wasn_resp {
            Ok(r) => r,
            Err(_) => wasm_bindings_name_t!(Response {
                status: 500,
                headers: None,
                body: None,
            }),
        }))
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

    async fn set_bool(
        &mut self,
        key: String,
        value: bool,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::Bool(value));
        Ok(Ok(()))
    }

    async fn set_s8(
        &mut self,
        key: String,
        value: i8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I8(value));
        Ok(Ok(()))
    }

    async fn set_s16(
        &mut self,
        key: String,
        value: i16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I16(value));
        Ok(Ok(()))
    }

    async fn set_s32(
        &mut self,
        key: String,
        value: i32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I32(value));
        Ok(Ok(()))
    }

    async fn set_s64(
        &mut self,
        key: String,
        value: i64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I64(value));
        Ok(Ok(()))
    }

    async fn set_u8(
        &mut self,
        key: String,
        value: u8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U8(value));
        Ok(Ok(()))
    }

    async fn set_u16(
        &mut self,
        key: String,
        value: u16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U16(value));
        Ok(Ok(()))
    }

    async fn set_u32(
        &mut self,
        key: String,
        value: u32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U32(value));
        Ok(Ok(()))
    }

    async fn set_u64(
        &mut self,
        key: String,
        value: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U64(value));
        Ok(Ok(()))
    }

    async fn set_f32(
        &mut self,
        key: String,
        value: f32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::F32(value));
        Ok(Ok(()))
    }

    async fn set_f64(
        &mut self,
        key: String,
        value: f64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::F64(value));
        Ok(Ok(()))
    }

    async fn set_char(
        &mut self,
        key: String,
        value: char,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::Char(value));
        Ok(Ok(()))
    }

    async fn set_string(
        &mut self,
        key: String,
        value: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::String(value));
        Ok(Ok(()))
    }

    async fn hset_bool(
        &mut self,
        key: String,
        field: String,
        value: bool,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::Bool(value));
        Ok(Ok(()))
    }

    async fn hset_s8(
        &mut self,
        key: String,
        field: String,
        value: i8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I8(value));
        Ok(Ok(()))
    }

    async fn hset_s16(
        &mut self,
        key: String,
        field: String,
        value: i16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I16(value));
        Ok(Ok(()))
    }

    async fn hset_s32(
        &mut self,
        key: String,
        field: String,
        value: i32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I32(value));
        Ok(Ok(()))
    }

    async fn hset_s64(
        &mut self,
        key: String,
        field: String,
        value: i64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I64(value));
        Ok(Ok(()))
    }

    async fn hset_u8(
        &mut self,
        key: String,
        field: String,
        value: u8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U8(value));
        Ok(Ok(()))
    }

    async fn hset_u16(
        &mut self,
        key: String,
        field: String,
        value: u16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U16(value));
        Ok(Ok(()))
    }

    async fn hset_u32(
        &mut self,
        key: String,
        field: String,
        value: u32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U32(value));
        Ok(Ok(()))
    }

    async fn hset_u64(
        &mut self,
        key: String,
        field: String,
        value: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U64(value));
        Ok(Ok(()))
    }

    async fn hset_f32(
        &mut self,
        key: String,
        field: String,
        value: f32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::F32(value));
        Ok(Ok(()))
    }

    async fn hset_f64(
        &mut self,
        key: String,
        field: String,
        value: f64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::F64(value));
        Ok(Ok(()))
    }

    async fn hset_char(
        &mut self,
        key: String,
        field: String,
        value: char,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::Char(value));
        Ok(Ok(()))
    }

    async fn hset_string(
        &mut self,
        key: String,
        field: String,
        value: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::String(value));
        Ok(Ok(()))
    }

    async fn get_bool(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<bool>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Bool(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_s8(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i8>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_s16(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i16>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_s32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_s64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_u8(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u8>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_u16(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u16>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_u32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_u64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_f32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<f32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_f64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<f64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_char(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<char>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Char(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_string(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::String(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_bool(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<bool>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Bool(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_s8(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i8>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_s16(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i16>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_s32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_s64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_u8(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u8>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_u16(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u16>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_u32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_u64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_f32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<f32>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_f64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<f64>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_char(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<char>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Char(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_string(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        let ms = self.stream_info.get().scc.get().ms.clone();
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(&ms).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::String(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
}

pub fn method_to_str(method: &wasm_bindings_name!(Method)) -> &'static str {
    match method {
        &<wasm_bindings_name!(Method)>::Get => "GET",
        &<wasm_bindings_name!(Method)>::Post => "POST",
        &<wasm_bindings_name!(Method)>::Put => "PUT",
        &<wasm_bindings_name!(Method)>::Delete => "DELETE",
        &<wasm_bindings_name!(Method)>::Patch => "PATCH",
        &<wasm_bindings_name!(Method)>::Head => "HEAD",
        &<wasm_bindings_name!(Method)>::Options => "OPTIONS",
    }
}

impl TryFrom<wasm_bindings_name!(Request)> for http::Request<hyper::Body> {
    type Error = anyhow::Error;

    fn try_from(wasm_req: wasm_bindings_name!(Request)) -> Result<Self, Self::Error> {
        let mut http_req = http::Request::builder()
            .method(http::Method::from_str(method_to_str(&wasm_req.method))?)
            .uri(&wasm_req.uri);

        for (key, value) in wasm_req.headers {
            http_req = http_req.header(key, value);
        }

        let body = match wasm_req.body {
            Some(b) => hyper::Body::from(b.to_vec()),
            None => hyper::Body::default(),
        };

        Ok(http_req.body(body)?)
    }
}

pub async fn http_response_to_wasn_response(
    mut http_res: http::Response<hyper::Body>,
) -> Result<wasm_bindings_name!(Response)> {
    let status = http_res.status().as_u16();
    let content_length = content_length(http_res.headers())?;
    let mut headers: Vec<(String, String)> = vec![];
    for (key, value) in http_res.headers() {
        headers.push((key.to_string(), value.to_str()?.to_string()));
    }
    let mut data = Vec::with_capacity(content_length as usize);
    loop {
        let body = http_res.body_mut().data().await;
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
    Ok(wasm_bindings_name_t!(Response {
        status: status,
        headers: Some(headers),
        body: Some(data),
    }))
}

pub struct ServerWasiView {
    table: ResourceTable,
    ctx: WasiCtx,
    wasm_host: WasmHost,
}

impl ServerWasiView {
    pub fn new(plugin: WasmHost) -> Self {
        let table = ResourceTable::new();
        let ctx = WasiCtxBuilder::new().inherit_stdio().build();
        Self {
            table,
            ctx,
            wasm_host: plugin,
        }
    }
    pub fn wasm_host(&mut self) -> &mut WasmHost {
        &mut self.wasm_host
    }
}

impl WasiView for ServerWasiView {
    fn table(&self) -> &ResourceTable {
        &self.table
    }

    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

pub async fn run_wasm_plugin(
    config: &str,
    plugin: WasmHost,
    wasm_plugin: &WasmPlugin,
) -> Result<crate::Error> {
    let mut linker = Linker::new(&wasm_plugin.engine);

    WasmServer::add_to_linker(&mut linker, ServerWasiView::wasm_host)
        .map_err(|e| anyhow!("err:WasmServer::add_to_linker => err:{}", e))?;

    // Add the command world (aka WASI CLI) to the linker
    command::add_to_linker(&mut linker)
        .map_err(|e| anyhow!("err:command::sync::add_to_linker => err:{}", e))?;

    let wasi_view = ServerWasiView::new(plugin);
    let mut store = Store::new(&wasm_plugin.engine, wasi_view);
    let (instance, _) = WasmServer::instantiate_async(&mut store, &wasm_plugin.component, &linker)
        .await
        .map_err(|e| anyhow!("err:instantiate => err:{}", e))?;

    use component::server::wasm_std;
    let ret: std::result::Result<wasm_std::Error, String> = instance
        .interface0
        .call_run(&mut store, config)
        .await
        .map_err(|e| anyhow!("err:call_add_headers => err:{}", e))?;
    if let Err(e) = &ret {
        return Err(anyhow::anyhow!("err:call_run => err{}", e));
    }
    return Ok(wasm_error_to_error(ret.unwrap()));
}

pub fn wasm_error_to_error(wasm_err: component::server::wasm_std::Error) -> crate::Error {
    use component::server::wasm_std;
    match wasm_err {
        wasm_std::Error::Ok => return crate::Error::Ok,
        wasm_std::Error::Break => return crate::Error::Break,
        wasm_std::Error::Finish => return crate::Error::Finish,
        wasm_std::Error::Error => return crate::Error::Error,
        wasm_std::Error::Ext1 => return crate::Error::Ext1,
        wasm_std::Error::Ext2 => return crate::Error::Ext2,
        wasm_std::Error::Ext3 => return crate::Error::Ext3,
    }
}

pub fn get_or_create_wash_hash_hash(
    key: String,
    wash_hash_hash: ArcRwLock<
        std::collections::HashMap<
            String,
            ArcRwLock<std::collections::HashMap<String, WasmHashValue>>,
        >,
    >,
) -> ArcRwLock<std::collections::HashMap<String, WasmHashValue>> {
    let wash_hash = wash_hash_hash.get().get(&key).cloned();
    let wash_hash = if wash_hash.is_some() {
        wash_hash.unwrap()
    } else {
        let wash_hash_hash = &mut wash_hash_hash.get_mut();
        let wash_hash = wash_hash_hash.get(&key).cloned();
        if wash_hash.is_some() {
            wash_hash.unwrap()
        } else {
            let wash_hash = ArcRwLock::new(std::collections::HashMap::new());
            wash_hash_hash.insert(key, wash_hash.clone());
            wash_hash
        }
    };
    wash_hash
}

pub fn get_wash_hash_value(
    key: String,
    field: String,
    wash_hash_hash: ArcRwLock<
        std::collections::HashMap<
            String,
            ArcRwLock<std::collections::HashMap<String, WasmHashValue>>,
        >,
    >,
) -> Option<WasmHashValue> {
    let value = wash_hash_hash.get().get(&key).cloned();
    if value.is_none() {
        return None;
    }
    let value = value.unwrap();
    let value = value.get().get(&field).cloned();
    value
}
