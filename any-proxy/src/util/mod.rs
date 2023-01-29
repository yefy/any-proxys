pub mod default_config;
pub mod domain_index;
pub mod openssl;
pub mod rustls;
pub mod signal;
pub mod util;
pub mod var;

use crate::config::config_toml::SSL;

pub struct SniContext {
    pub index: i32,
    pub ssl: Option<SSL>,
}
