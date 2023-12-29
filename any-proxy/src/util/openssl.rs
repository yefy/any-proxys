#![cfg(feature = "anyproxy-openssl")]

use super::cache;
use super::domain_index::DomainIndex;
use super::SniContext;
use any_base::typ::{ArcMutex, ArcRwLock};
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use openssl::error::ErrorStack;
use openssl::ex_data::Index;
use openssl::ssl::Ssl;
use openssl::ssl::SslAcceptor;
use openssl::ssl::{self, AlpnError};
use openssl::ssl::{SslContext, SslContextBuilder};
use openssl::ssl::{SslFiletype, SslMethod};
use std::collections::HashMap;
use std::sync::Arc;

lazy_static! {
    pub static ref SESSION_CACHE: ArcMutex<cache::SessionCache> =
        ArcMutex::new(cache::SessionCache::new());
}

pub fn session_cache() -> ArcMutex<cache::SessionCache> {
    SESSION_CACHE.clone()
}

pub fn key_index() -> Result<Index<Ssl, cache::SessionKey>, ErrorStack> {
    static IDX: OnceCell<Index<Ssl, cache::SessionKey>> = OnceCell::new();
    IDX.get_or_try_init(Ssl::new_ex_index).map(|v| *v)
}

/// opensll sni对象
pub struct OpensslSni {
    pub default_key: ArcRwLock<String>,
    pub default_cert: ArcRwLock<String>,
    pub sni_map: ArcRwLock<HashMap<i32, SslContext>>,
    pub domain_index: ArcRwLock<DomainIndex>,
}

impl OpensslSni {
    pub fn new(
        ctxs: &Vec<SniContext>,
        domain_index: DomainIndex,
        prototol: &str,
    ) -> Result<OpensslSni> {
        let mut sni_map = HashMap::new();

        let mut default_key = "".to_string();
        let mut default_cert = "".to_string();

        for ctx in ctxs.iter() {
            if ctx.ssl.is_none() {
                continue;
            }

            let ssl = ctx.ssl.as_ref().unwrap();
            if default_key.len() <= 0 {
                default_key = ssl.key.clone();
                default_cert = ssl.cert.clone();
            }

            let mut ssl_context = SslContextBuilder::new(SslMethod::tls())
                .map_err(|e| anyhow!("err:SslContextBuilder::new => e:{}", e))?;
            ssl_context
                .set_private_key_file(&ssl.key, SslFiletype::PEM)
                .map_err(|e| anyhow!("err:ssl_context.set_private_key_file => e:{}", e))?;
            ssl_context
                .set_certificate_chain_file(&ssl.cert)
                .map_err(|e| anyhow!("err:ssl_context.set_certificate_chain_file => e:{}", e))?;

            if prototol.len() > 0 {
                let prototol = prototol.to_string();
                ssl_context.set_alpn_select_callback(move |_, client| {
                    //ssl::select_next_proto(b"\x02h2", client).ok_or(AlpnError::NOACK)
                    //ssl::select_next_proto(b"\x02h2\x08http/1.1", client).ok_or(AlpnError::NOACK)
                    //ssl.set_alpn_protos(b"\x02h2\x08http/1.1")?;
                    ssl::select_next_proto(prototol.as_bytes(), client).ok_or(AlpnError::NOACK)
                });
            }

            let context = ssl_context.build();
            sni_map.insert(ctx.index, context);
        }

        Ok(OpensslSni {
            default_key: ArcRwLock::new(default_key),
            default_cert: ArcRwLock::new(default_cert),
            sni_map: ArcRwLock::new(sni_map),
            domain_index: ArcRwLock::new(domain_index),
        })
    }

    /// reload的时候，clone新配置
    pub fn take_from(&self, other: &OpensslSni) {
        self.default_key.set(unsafe { other.default_key.take() });
        self.default_cert.set(unsafe { other.default_cert.take() });
        self.sni_map.set(unsafe { other.sni_map.take() });
        self.domain_index.set(unsafe { other.domain_index.take() });
    }

    ///openssl 加密accept
    pub fn tls_acceptor(&self, prototol: &[u8]) -> Result<Arc<SslAcceptor>> {
        let sni_map = self.sni_map.clone();
        let mut acceptor = SslAcceptor::mozilla_modern(SslMethod::tls())
            .map_err(|e| anyhow!("err:SslAcceptor::mozilla_modern => e:{}", e))?;
        acceptor
            .set_certificate_chain_file(self.default_cert.get().as_str())
            .map_err(|e| anyhow!("err:set_certificate_chain_file => e:{}", e))?;
        acceptor
            .set_private_key_file(self.default_key.get().as_str(), SslFiletype::PEM)
            .map_err(|e| anyhow!("err:set_private_key_file => e:{}", e))?;
        if prototol.len() > 0 {
            let prototol = prototol.to_vec();
            acceptor.set_alpn_select_callback(move |_, client| {
                //ssl::select_next_proto(b"\x02h2", client).ok_or(AlpnError::NOACK)
                //ssl::select_next_proto(b"\x02h2\x08http/1.1", client).ok_or(AlpnError::NOACK)
                //ssl.set_alpn_protos(b"\x02h2\x08http/1.1")?;
                ssl::select_next_proto(&prototol, client).ok_or(AlpnError::NOACK)
            });
        }

        let domain_index = self.domain_index.clone();
        let context_builder = &mut *acceptor;
        context_builder.set_servername_callback(
            move |ssl, alert| -> std::result::Result<(), openssl::ssl::SniError> {
                let domain = ssl
                    .servername(openssl::ssl::NameType::HOST_NAME)
                    .ok_or_else(|| {
                        log::error!("{}", "err:openssl servername nil");
                        *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                        openssl::ssl::SniError::ALERT_FATAL
                    })?;

                log::debug!("domain:{}", domain);
                let domain_index = domain_index.get().index(domain).map_err(|_| {
                    log::error!("{}", "err:openssl cert nil");
                    *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                    openssl::ssl::SniError::ALERT_FATAL
                })?;

                log::debug!("domain_index:{}", domain_index);
                let sni_map = sni_map.get();
                let ssl_context = sni_map.get(&domain_index).ok_or_else(|| {
                    *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                    openssl::ssl::SniError::ALERT_FATAL
                })?;

                // {
                //     ssl.servername(openssl::ssl::NameType::HOST_NAME)
                //         .ok_or_else(|| {
                //             log::error!("{}", "err:openssl servername nil");
                //             *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                //             openssl::ssl::SniError::ALERT_FATAL
                //         })
                //         .and_then(|domain| {
                //             log::debug!("domain:{}", domain);
                //             domain_index.read().unwrap().as_ref().unwrap()
                //                 .index(domain)
                //                 .map_err(|_| {
                //                     log::error!("{}", "err:openssl cert nil");
                //                     *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                //                     openssl::ssl::SniError::ALERT_FATAL
                //                 })
                //                 .and_then(|domain_index| {
                //                     log::debug!("domain_index:{}", domain_index);
                //                     let sni_map = sni_map.read().unwrap();
                //                     sni_map.as_ref().unwrap().get(&domain_index).ok_or_else(|| {
                //                         *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                //                         openssl::ssl::SniError::ALERT_FATAL
                //                     })
                //                 })
                //         })?
                // }

                ssl.set_ssl_context(ssl_context).map_err(|e| {
                    log::error!("err:set_servername_callback => e:{}", e);
                    openssl::ssl::SniError::ALERT_FATAL
                })?;
                Ok(())
            },
        );
        let tls_acceptor = Arc::new(acceptor.build());
        Ok(tls_acceptor)
    }
}
