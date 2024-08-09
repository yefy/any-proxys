use crate::proxy::stream_info::StreamInfo;
use crate::stream::server::ServerStreamInfo;
use crate::Protocol7;
use any_base::typ::ArcRwLock;
use lazy_static::lazy_static;
use std::env;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

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
    pub static ref ANYPROXY_TMP_FULL_PATH: ArcRwLock<String> = ArcRwLock::new("./tmp/".to_string());
}

lazy_static! {
    pub static ref PAGE_SIZE: AtomicUsize = AtomicUsize::new(4096);
}

lazy_static! {
    pub static ref HOT_PID: ArcRwLock<String> = ArcRwLock::default();
}

pub fn build_version() -> String {
    format!(
        "{}/{} ({} {})",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        get_vergen_git_sha_short(),
        get_vergen_git_commit_date()
    )
}

#[allow(dead_code)]
fn get_vergen_build_semver() -> String {
    match env::var("VERGEN_BUILD_SEMVER") {
        Ok(val) => val,
        Err(_) => "0.0.0".to_string(),
    }
}

fn get_vergen_git_sha_short() -> String {
    match env::var("VERGEN_GIT_SHA_SHORT") {
        Ok(val) => val,
        Err(_) => "000".to_string(),
    }
}

fn get_vergen_git_commit_date() -> String {
    match env::var("VERGEN_GIT_COMMIT_DATE") {
        Ok(val) => val,
        Err(_) => "000".to_string(),
    }
}

lazy_static! {
    pub static ref BUILD_VERSION: String = build_version();
}

pub fn http_version() -> String {
    format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),)
}

lazy_static! {
    pub static ref HTTP_VERSION: String = http_version();
}

lazy_static! {
    pub static ref VAR_STREAM_INFO: StreamInfo = StreamInfo::new(
        Arc::new(ServerStreamInfo {
            protocol7: Protocol7::Tcp,
            remote_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            local_addr: Some(SocketAddr::from(([127, 0, 0, 1], 18080))),
            domain: None,
            is_tls: false,
            raw_fd: 0,
            listen_shutdown_tx: None.into(),
            listen_worker: None.into(),
        }),
        false,
        None,
        0,
        0,
        0,
        false,
        1001,
        ArcRwLock::default(),
    );
}
