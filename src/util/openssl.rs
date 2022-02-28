#![cfg(feature = "anyproxy-openssl")]

use super::domain_index::DomainIndex;
use super::SniContext;
use anyhow::Result;
use openssl::ssl::SslAcceptor;
use openssl::ssl::{self, AlpnError};
use openssl::ssl::{SslContext, SslContextBuilder};
use openssl::ssl::{SslFiletype, SslMethod};
use std::collections::HashMap;
use std::rc::Rc;

/// opensll sni对象
pub struct OpensslSni {
    pub default_key: String,
    pub default_cert: String,
    pub sni_map: std::sync::Arc<HashMap<i32, SslContext>>,
}

impl OpensslSni {
    pub fn new(ctxs: &Vec<SniContext>, prototol: &str) -> Result<OpensslSni> {
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

            let mut ssl_context = SslContextBuilder::new(SslMethod::tls())?;
            ssl_context.set_private_key_file(&ssl.key, SslFiletype::PEM)?;
            ssl_context.set_certificate_chain_file(&ssl.cert)?;

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
            default_key,
            default_cert,
            sni_map: std::sync::Arc::new(sni_map),
        })
    }
    ///openssl 加密accept
    pub fn tls_acceptor(
        &self,
        domain_index: std::sync::Arc<DomainIndex>,
        prototol: &str,
    ) -> Result<Rc<SslAcceptor>> {
        let sni_map = self.sni_map.clone();
        let mut acceptor = SslAcceptor::mozilla_modern(SslMethod::tls())?;
        acceptor.set_certificate_chain_file(&self.default_cert)?;
        acceptor.set_private_key_file(&self.default_key, SslFiletype::PEM)?;
        if prototol.len() > 0 {
            let prototol = prototol.to_string();
            acceptor.set_alpn_select_callback(move |_, client| {
                //ssl::select_next_proto(b"\x02h2", client).ok_or(AlpnError::NOACK)
                //ssl::select_next_proto(b"\x02h2\x08http/1.1", client).ok_or(AlpnError::NOACK)
                //ssl.set_alpn_protos(b"\x02h2\x08http/1.1")?;
                ssl::select_next_proto(prototol.as_bytes(), client).ok_or(AlpnError::NOACK)
            });
        }

        let context_builder = &mut *acceptor;
        context_builder.set_servername_callback(
            move |ssl, alert| -> std::result::Result<(), openssl::ssl::SniError> {
                ssl.set_ssl_context({
                    ssl.servername(openssl::ssl::NameType::HOST_NAME)
                        .ok_or_else(|| {
                            log::error!("{}", "err:openssl servername nil");
                            *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                            openssl::ssl::SniError::ALERT_FATAL
                        })
                        .and_then(|domain| {
                            log::debug!("domain:{}", domain);
                            domain_index
                                .index(domain)
                                .map_err(|_| {
                                    log::error!("{}", "err:openssl cert nil");
                                    *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                                    openssl::ssl::SniError::ALERT_FATAL
                                })
                                .and_then(|domain_index| {
                                    log::debug!("domain_index:{}", domain_index);
                                    sni_map.get(&domain_index).ok_or_else(|| {
                                        *alert = openssl::ssl::SslAlert::UNRECOGNIZED_NAME.clone();
                                        openssl::ssl::SniError::ALERT_FATAL
                                    })
                                })
                        })?
                })
                .map_err(|e| {
                    log::error!("err:set_servername_callback => e:{}", e);
                    openssl::ssl::SniError::ALERT_FATAL
                })?;
                Ok(())
            },
        );
        let tls_acceptor = Rc::new(acceptor.build());
        Ok(tls_acceptor)
    }
}
