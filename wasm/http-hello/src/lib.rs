#[allow(warnings)]
mod bindings;
mod component;
mod macros;
mod service;
mod util;

pub use crate::bindings::component::server::wasm_http;
pub use crate::bindings::component::server::wasm_log;
pub use crate::bindings::component::server::wasm_socket;
pub use crate::bindings::component::server::wasm_std;
pub use crate::bindings::component::server::wasm_store;
pub use crate::bindings::component::server::wasm_websocket;

pub struct Component {}

bindings::export!(Component with_types_in bindings);
