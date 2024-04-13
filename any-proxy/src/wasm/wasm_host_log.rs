use crate::wasm::component::server::wasm_log;
use crate::wasm::WasmHost;
use async_trait::async_trait;

#[async_trait]
impl wasm_log::Host for WasmHost {
    async fn log_enabled(&mut self, level: wasm_log::Level) -> wasmtime::Result<bool> {
        let level = match level {
            wasm_log::Level::Error => log::Level::Error,
            wasm_log::Level::Warn => log::Level::Warn,
            wasm_log::Level::Info => log::Level::Info,
            wasm_log::Level::Debug => log::Level::Debug,
            wasm_log::Level::Trace => log::Level::Trace,
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
}
