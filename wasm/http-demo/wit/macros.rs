#[macro_export]
macro_rules! wasm_bindings_name {
    ($name:ident) => {
        crate::bindings::component::server::wasm_std::$name
    };
}

#[macro_export]
macro_rules! bindings_ename {
    ($name:ident) => {
        crate::bindings::exports::component::server::wasm_service::$name
    };
}

#[macro_export]
macro_rules! error {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_name!(log_enabled)(<wasm_bindings_name!(Level)>::Error) {
            wasm_bindings_name!(log_error)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! warn {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_name!(log_enabled)(<wasm_bindings_name!(Level)>::Warn) {
            wasm_bindings_name!(log_warn)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! info {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_name!(log_enabled)(<wasm_bindings_name!(Level)>::Info) {
            wasm_bindings_name!(log_info)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_name!(log_enabled)(<wasm_bindings_name!(Level)>::Debug) {
            wasm_bindings_name!(log_debug)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($format:expr $(, $args:expr)*) => {
        if wasm_bindings_name!(log_enabled)(<wasm_bindings_name!(Level)>::Trace) {
            wasm_bindings_name!(log_trace)(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
        }
    };
}
