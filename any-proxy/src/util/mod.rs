pub mod cache;
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

#[derive(Clone)]
pub struct Sni {
    pub sni_rustls: std::sync::Arc<rustls::ResolvesServerCertUsingSNI>,
    #[cfg(feature = "anyproxy-openssl")]
    pub sni_openssl: std::sync::Arc<openssl::OpensslSni>,
}

impl Sni {
    pub fn take_from(&self, sni: &Sni) {
        self.sni_rustls.take_from(&sni.sni_rustls);
        #[cfg(feature = "anyproxy-openssl")]
        self.sni_openssl.take_from(&sni.sni_openssl);
    }
}
