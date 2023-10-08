use super::util;
use crate::config::config_toml::TcpConfig as Config;
use crate::ssl::stream::{Stream, StreamData};
use crate::stream::client;
use crate::stream::stream_flow;
use crate::Protocol7;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;

pub struct Client {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Arc<Config>,
    ssl_domain: String,
}

impl Client {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Arc<Config>,
        ssl_domain: String,
    ) -> Result<Client> {
        Ok(Client {
            addr,
            timeout,
            config,
            ssl_domain,
        })
    }
}

#[async_trait]
impl client::Client for Client {
    async fn connect(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<Box<dyn client::Connection + Send>> {
        let stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        if info.is_some() {
            info.as_mut().unwrap().err = stream_err;
        }
        Ok(Box::new(Connection::new(
            self.addr.clone(),
            self.timeout.clone(),
            self.config.clone(),
            self.ssl_domain.clone(),
        )?))
    }
}

pub struct Connection {
    addr: SocketAddr,
    timeout: tokio::time::Duration,
    config: Arc<Config>,
    ssl_domain: String,
}

impl Connection {
    pub fn new(
        addr: SocketAddr,
        timeout: tokio::time::Duration,
        config: Arc<Config>,
        ssl_domain: String,
    ) -> Result<Connection> {
        Ok(Connection {
            addr,
            timeout,
            config,
            ssl_domain,
        })
    }
}

#[async_trait]
impl client::Connection for Connection {
    async fn stream(
        &self,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(Protocol7, stream_flow::StreamFlow, SocketAddr, SocketAddr)> {
        let mut stream_err: stream_flow::StreamFlowErr = stream_flow::StreamFlowErr::Init;
        let ret: Result<TcpStream> = async {
            match tokio::time::timeout(self.timeout, TcpStream::connect(&self.addr)).await {
                Ok(ret) => match ret {
                    Ok(stream) => Ok(stream),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                        Err(anyhow!("err:client.stream timeout => addr:{}", self.addr))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow!("err:client.stream reset => addr:{}", self.addr))
                    }
                    Err(e) => {
                        stream_err = stream_flow::StreamFlowErr::WriteErr;
                        Err(anyhow!(
                            "err:client.stream => addr:{}, kind:{:?}, e:{}",
                            self.addr,
                            e.kind(),
                            e
                        ))
                    }
                },
                Err(_) => {
                    stream_err = stream_flow::StreamFlowErr::WriteTimeout;
                    Err(anyhow!("err:client.stream timeout => addr:{}", self.addr))
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if info.is_some() {
                    info.as_mut().unwrap().err = stream_err;
                }
                Err(e)
            }
            Ok(tcp_stream) => {
                util::set_stream(&tcp_stream, &self.config);
                let local_addr = tcp_stream
                    .local_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.local_addr => e:{}", e))?;
                let remote_addr = tcp_stream
                    .peer_addr()
                    .map_err(|e| anyhow!("err:tcp_stream.peer_addr => e:{}", e))?;

                //#[cfg(unix)]
                //use std::os::unix::io::AsRawFd;
                //#[cfg(unix)]
                //let fd = tcp_stream.as_raw_fd();
                //#[cfg(not(unix))]
                let fd = 0;

                #[cfg(feature = "anyproxy-openssl")]
                {
                    use crate::util::cache as util_cache;
                    use crate::util::openssl as util_openssl;
                    use openssl::ssl::SslSessionCacheMode;
                    use parking_lot::Mutex;
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
                    //ssl.set_alpn_protos(alpn_protos.as_bytes())?;
                    let cache = Arc::new(Mutex::new(util_cache::SessionCache::new()));
                    ssl.set_session_cache_mode(SslSessionCacheMode::CLIENT);
                    ssl.set_new_session_callback({
                        let cache = cache.clone();
                        move |ssl, session| {
                            if let Some(key) = util_openssl::key_index()
                                .ok()
                                .and_then(|idx| ssl.ex_data(idx))
                            {
                                cache.lock().insert(key.clone(), session);
                            }
                        }
                    });
                    ssl.set_remove_session_callback({
                        let cache = cache.clone();
                        move |_, session| cache.lock().remove(session)
                    });
                    let ssl = ssl.build();
                    let mut conf = ssl.configure()?;
                    let key = util_cache::SessionKey {
                        host: self.ssl_domain.clone(),
                        port: self.addr.port(),
                    };
                    if let Some(session) = cache.lock().get(&key) {
                        unsafe {
                            conf.set_session(&session)?;
                        }
                    }
                    let idx = util_openssl::key_index()?;
                    conf.set_ex_data(idx, key);
                    let ssl = conf.into_ssl(&self.ssl_domain)?;
                    let mut ssl_stream = SslStream::new(ssl, tcp_stream)?;
                    std::pin::Pin::new(&mut ssl_stream).connect().await?;
                    let stream = Stream::new(StreamData::Openssl(ssl_stream));
                    let (r, w) = any_base::io::split::split(stream);
                    let stream = stream_flow::StreamFlow::new(fd, Box::new(r), Box::new(w));
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
                    client_crypto.alpn_protocols = vec![];
                    client_crypto.key_log = Arc::new(rustls::KeyLogFile::new());

                    let connector = TlsConnector::from(Arc::new(client_crypto));
                    let domain =
                        rustls::ServerName::try_from(self.ssl_domain.as_str()).map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidInput, "invalid dnsname")
                        })?;

                    let ssl_stream = connector
                        .connect(domain, tcp_stream)
                        .await
                        .map_err(|e| anyhow!("err:connector.connect => e:{}", e))?;

                    let stream = Stream::new(StreamData::C(ssl_stream));
                    let (r, w) = any_base::io::split::split(stream);
                    let stream = stream_flow::StreamFlow::new(fd, Box::new(r), Box::new(w));
                    Ok((Protocol7::Ssl, stream, local_addr, remote_addr))
                }
            }
        }
    }
}