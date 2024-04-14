#[macro_export]
macro_rules! error {
    ($format:expr $(, $args:expr)*) => {
        {
            use crate::wasm_log;
            if wasm_log::log_enabled(wasm_log::Level::Error) {
                wasm_log::log_error(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
            }
        }
    };
}

#[macro_export]
macro_rules! warn {
    ($format:expr $(, $args:expr)*) => {
        {
            use crate::wasm_log;
            if wasm_log::log_enabled(wasm_log::Level::Warn) {
                wasm_log::log_warn(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
            }
        }
    };
}

#[macro_export]
macro_rules! info {
    ($format:expr $(, $args:expr)*) => {
        {
            use crate::wasm_log;
            if wasm_log::log_enabled(wasm_log::Level::Info) {
                wasm_log::log_info(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
            }
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($format:expr $(, $args:expr)*) => {
        {
            use crate::wasm_log;
            if wasm_log::log_enabled(wasm_log::Level::Debug) {
                wasm_log::log_debug(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
            }
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($format:expr $(, $args:expr)*) => {
        {
            use crate::wasm_log;
            if wasm_log::log_enabled(wasm_log::Level::Trace) {
                wasm_log::log_trace(&format!("{}:{} {}", file!(), line!(), format!($format $(, $args)*)))?;
            }
        }
    };
}
