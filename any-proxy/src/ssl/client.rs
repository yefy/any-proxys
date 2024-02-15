use super::util;
use crate::config::config_toml::TcpConfig as Config;
use crate::ssl::stream::{Stream, StreamData};
use crate::stream::client;
use crate::Protocol7;
use any_base::stream_flow::{StreamFlow, StreamFlowErr, StreamFlowInfo};
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;

pub struct ClientContext {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Arc<Config>,
    ssl_domain: String,
}

pub struct Client {
    context: Arc<ClientContext>,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Arc<Config>,
        ssl_domain: String,
    ) -> Result<Client> {
        Ok(Client {
            context: Arc::new(ClientContext {
                addr,
                timeout,
                config,
                ssl_domain,
            }),
        })
    }
}

#[async_trait]
impl client::Client for Client {
    async fn connect(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<Box<dyn client::Connection + Send>> {
        let stream_err: StreamFlowErr = StreamFlowErr::Init;
        if info.is_some() {
            info.as_ref().unwrap().get_mut().err = stream_err;
        }
        Ok(Box::new(Connection::new(self.context.clone())?))
    }
}

pub struct Connection {
    context: Arc<ClientContext>,
}

impl Connection {
    pub fn new(context: Arc<ClientContext>) -> Result<Connection> {
        Ok(Connection { context })
    }
}

#[async_trait]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) -> Result<(Protocol7, StreamFlow, SocketAddr, SocketAddr)> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.context.timeout, TcpStream::connect(&self.context.addr))
                .await
            {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        stream_err = StreamFlowErr::WriteTimeout;
                        Err(anyhow!(
                            "err:client.stream timeout => addr:{}",
                            self.context.addr
                        ))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream reset => addr:{}",
                            self.context.addr
                        ))
                    }
                    Err(e) => {
                        stream_err = StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream => addr:{}, kind:{:?}, e:{}",
                            self.context.addr,
                            e.kind(),
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    Err(anyhow!(
                        "err:client.stream timeout => addr:{}",
                        self.context.addr
                    ))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if info.is_some() {
                    info.as_ref().unwrap().get_mut().err = stream_err;
                }
                Err(e)
            }
            Ok(tcp_stream) => {
                util::set_stream(&tcp_stream, &self.context.config);
                let local_addr = tcp_stream
                    .local_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.local_addr => e:{}", e))?;
                let remote_addr = tcp_stream
                    .peer_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.peer_addr => e:{}", e))?;

                #[cfg(feature = "anyproxy-openssl")]
                {
                    use crate::util::cache as util_cache;
                    use crate::util::openssl as util_openssl;
                    use openssl::ssl::SslSessionCacheMode;
                    use tokio_openssl::SslStream;
                    let is_insecure = true;
                    let mut ssl_verify_mode = openssl::ssl::SslVerifyMode::PEER;
                    if is_insecure {
                        ssl_verify_mode = openssl::ssl::SslVerifyMode::NONE;
                    }
                    let mut ssl =
                        openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls())?;
                    ssl.set_verify(ssl_verify_mode);
                    //ssl.set_verify(SslVerifyMode::FAIL_IF_NO_PEER_CERT);
                    //ssl.set_ca_file("cert/www.yefyyun.cn.pem")?;
                    ssl.set_alpn_protos(b"\x02h2\x08http/1.1")?;
                    //let cache = ArcMutex::new(util_cache::SessionCache::new());
                    let cache = util_openssl::session_cache();
                    ssl.set_session_cache_mode(SslSessionCacheMode::CLIENT);
                    ssl.set_new_session_callback({
                        let cache = cache.clone();
                        move |ssl, session| {
                            if let Some(key) = util_openssl::key_index()
                                .ok()
                                .and_then(|idx| ssl.ex_data(idx))
                            {
                                cache.get_mut().insert(key.clone(), session);
                            }
                        }
                    });
                    ssl.set_remove_session_callback({
                        let cache = cache.clone();
                        move |_, session| cache.get_mut().remove(session)
                    });
                    let ssl = ssl.build();
                    let mut conf = ssl.configure()?;
                    let key = util_cache::SessionKey {
                        host: self.context.ssl_domain.clone(),
                        port: self.context.addr.port(),
                    };
                    if let Some(session) = cache.get_mut().get(&key) {
                        unsafe {
                            conf.set_session(&session)?;
                        }
                    }
                    let idx = util_openssl::key_index()?;
                    conf.set_ex_data(idx, key);
                    let ssl = conf.into_ssl(&self.context.ssl_domain)?;
                    let mut ssl_stream = SslStream::new(ssl, tcp_stream)?;
                    std::pin::Pin::new(&mut ssl_stream).connect().await?;
                    let stream = Stream::new(StreamData::Openssl(ssl_stream));
                    let stream = StreamFlow::new(stream, Some(vec!["shut down".to_string()]));
                    Ok((Protocol7::Ssl, stream, local_addr, remote_addr))
                }

                #[cfg(feature = "anyproxy-rustls")]
                {
                    use crate::util::rustls as util_rustls;
                    use std::convert::TryFrom;
                    use tokio_rustls::rustls::{self};
                    use tokio_rustls::TlsConnector;

                    let mut client_crypto =
                        rustls::ClientConfig::builder()
                            .with_safe_defaults()
                            .with_custom_certificate_verifier(
                                util_rustls::SkipServerVerification::new(),
                            )
                            .with_no_client_auth();
                    client_crypto.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
                    client_crypto.key_log = Arc::new(rustls::KeyLogFile::new());

                    let connector = TlsConnector::from(Arc::new(client_crypto));
                    let domain = rustls::ServerName::try_from(self.context.ssl_domain.as_str())
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidInput, "invalid dnsname")
                        })?;

                    let ssl_stream = connector
                        .connect(domain, tcp_stream)
                        .await
                        .map_err(|e| anyhow!("err:connector.connect => e:{}", e))?;

                    let stream = Stream::new(StreamData::C(ssl_stream));
                    let stream = StreamFlow::new(stream, Some(vec!["shut down".to_string()]));
                    Ok((Protocol7::Ssl, stream, local_addr, remote_addr))
                }
            }
        }
    }
}
