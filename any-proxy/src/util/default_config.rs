use lazy_static::lazy_static;

lazy_static! {
    pub static ref ANYPROXY_PID_FULL_PATH: String = "./logs/anyproxy.pid".to_string();
}

lazy_static! {
    pub static ref ANYPROXY_SIGNAL_FULL_PATH: String = "./logs/anyproxy.signal".to_string();
}

lazy_static! {
    pub static ref ANYPROXY_LOG_FULL_PATH: String = "./conf/log4rs.yaml".to_string();
}

lazy_static! {
    pub static ref ANYPROXY_CONF_FULL_PATH: String = "./conf/anyproxy.conf".to_string();
}

lazy_static! {
    pub static ref ANYPROXY_CONF_PATH: String = "./conf/".to_string();
}

lazy_static! {
    pub static ref ANYPROXY_CONF_LOG_FULL_PATH: String = "./logs/anyproxy.toml".to_string();
}
