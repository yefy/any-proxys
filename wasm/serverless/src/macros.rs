#[macro_export]
macro_rules! wasm_bindings_std {
    ($name:ident) => {
        crate::bindings::component::server::wasm_std::$name
    };
}

#[macro_export]
macro_rules! wasm_bindings_log {
    ($name:ident) => {
        crate::bindings::component::server::wasm_log::$name
    };
}

#[macro_export]
macro_rules! wasm_bindings_service {
    ($name:ident) => {
        crate::bindings::exports::component::server::wasm_service::$name
    };
}

#[macro_export]
macro_rules! error {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_log!(log_enabled)(<wasm_bindings_log!(Level)>::Error) {
            wasm_bindings_log!(log_error)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! warn {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_log!(log_enabled)(<wasm_bindings_log!(Level)>::Warn) {
            wasm_bindings_log!(log_warn)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! info {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_log!(log_enabled)(<wasm_bindings_log!(Level)>::Info) {
            wasm_bindings_log!(log_info)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_log!(log_enabled)(<wasm_bindings_log!(Level)>::Debug) {
            wasm_bindings_log!(log_debug)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_log!(log_enabled)(<wasm_bindings_log!(Level)>::Trace) {
            wasm_bindings_log!(log_trace)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}
