use any_base::typ::ArcRwLock;
use lazy_static::lazy_static;
use std::sync::atomic::AtomicUsize;

lazy_static! {
    pub static ref ANYPROXY_PID_FULL_PATH: ArcRwLock<String> =
        ArcRwLock::new("./logs/anyproxy.pid".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_SIGNAL_FULL_PATH: ArcRwLock<String> =
        ArcRwLock::new("./logs/anyproxy.signal".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_LOG_FULL_PATH: ArcRwLock<String> =
        ArcRwLock::new("./conf/log4rs.yaml".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_FULL_PATH: ArcRwLock<String> =
        ArcRwLock::new("./conf/anyproxy.conf".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_PATH: ArcRwLock<String> = ArcRwLock::new("./conf/".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_LOG_RAW_PATH: ArcRwLock<String> =
        ArcRwLock::new("./logs/anyproxy.conf".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_LOG_FULL_PATH: ArcRwLock<String> =
        ArcRwLock::new("./logs/anyproxy.toml".to_string());
}

lazy_static! {
    pub static ref PAGE_SIZE: AtomicUsize = AtomicUsize::new(4096);
}

lazy_static! {
    pub static ref HOT_PID: ArcRwLock<String> = ArcRwLock::default();
}
