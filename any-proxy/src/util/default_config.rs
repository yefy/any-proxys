use lazy_static::lazy_static;
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

lazy_static! {
    pub static ref ANYPROXY_PID_FULL_PATH: Mutex<String> =
        Mutex::new("./logs/anyproxy.pid".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_SIGNAL_FULL_PATH: Mutex<String> =
        Mutex::new("./logs/anyproxy.signal".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_LOG_FULL_PATH: Mutex<String> =
        Mutex::new("./conf/log4rs.yaml".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_FULL_PATH: Mutex<String> =
        Mutex::new("./conf/anyproxy.conf".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_PATH: Mutex<String> = Mutex::new("./conf/".to_string());
}

lazy_static! {
    pub static ref ANYPROXY_CONF_LOG_FULL_PATH: Mutex<String> =
        Mutex::new("./logs/anyproxy.toml".to_string());
}

lazy_static! {
    pub static ref PAGE_SIZE: AtomicUsize = AtomicUsize::new(4096);
}

lazy_static! {
    pub static ref MIN_CACHE_BUFFER_NUM: usize = 4;
}
