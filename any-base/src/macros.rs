#[derive(Clone)]
pub enum Runtime {
    LocalRuntime = 0,
    ThreadRuntime,
}

#[macro_export]
macro_rules! spawn {
    ($runtime:expr, $future:expr) => {{
        match $runtime {
            Runtime::LocalRuntime => tokio::task::spawn_local($future),
            Runtime::ThreadRuntime => tokio::spawn($future),
        }
    }};
}
