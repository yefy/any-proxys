#[macro_export]
macro_rules! wasm_bindings_name {
    ($name:ident) => {
        component::server::wasm_std::$name
    };
}

#[macro_export]
macro_rules! wasm_bindings_name_t {
    ($name:ident { $($field:ident : $value:expr),* $(,)? }) => {
        component::server::wasm_std::$name {
            $($field: $value),*
        }
    };
}

#[macro_export]
macro_rules! wasm_bind_server {
    ($path:expr, $path2:expr) => {
        wasmtime::component::bindgen!({
            path: $path,
            world: "wasm-server",
            async: true,
        });

        include!($path2);
    };
}
