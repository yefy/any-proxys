use super::util as tcp_util;
use crate::config::config_toml::TcpConfig as Config;
use crate::ssl::stream::{Stream, StreamData};
use crate::stream::server;
use crate::stream::server::ServerStreamInfo;
use crate::util;
use crate::Protocol7;
use any_base::io::async_stream::Stream as IoStream;
use any_base::stream_flow::StreamFlow;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

pub struct Server {
    addr: SocketAddr,
    reuseport: bool,
    config: Arc<Config>,
    sni: ArcMutex<util::Sni>,
}

impl Server {
    pub fn new(
        addr: SocketAddr,
        reuseport: bool,
        config: Arc<Config>,
        sni: util::Sni,
    ) -> Result<Server> {
        Ok(Server {
            addr,
            reuseport,
            config,
            sni: ArcMutex::new(sni),
        })
    }
}

#[async_trait]
impl server::Server for Server {
    fn stream_send_timeout(&self) -> usize {
        return self.config.tcp_send_timeout;
    }
    fn stream_recv_timeout(&self) -> usize {
        return self.config.tcp_recv_timeout;
    }

    async fn listen(&self) -> Result<Box<dyn server::Listener>> {
        let std_listener = tcp_util::bind(&self.addr, self.reuseport)
            .map_err(|e| anyhow!("err:tcp_util::bind => e:{}", e))?;
        let listener = TcpListener::from_std(std_listener)
            .map_err(|e| anyhow!("err:TcpListener::from_std => e:{}", e))?;
        Ok(Box::new(
            Listener::new(listener, self.config.clone(), self.sni().unwrap())
                .map_err(|e| anyhow!("err:Listener::new => e:{}", e))?,
        ))
    }
    fn listen_addr(&self) -> Result<SocketAddr> {
        Ok(self.addr.clone())
    }
    fn sni(&self) -> Option<util::Sni> {
        Some(self.sni.get().clone())
    }
    fn set_sni(&self, sni: util::Sni) {
        self.sni.set(sni);
    }

    fn protocol7(&self) -> Protocol7 {
        Protocol7::Ssl
    }
    fn is_tls(&self) -> bool {
        true
    }
}

pub struct Listener {
    listener: TcpListener,
    config: Arc<Config>,
    sni: util::Sni,
}

impl Listener {
    pub fn new(listener: TcpListener, config: Arc<Config>, sni: util::Sni) -> Result<Listener> {
        Ok(Listener {
            listener,
            config,
            sni,
        })
    }
}

#[async_trait]
impl server::Listener for Listener {
    async fn accept(&mut self) -> Result<(Box<dyn server::Connection>, bool)> {
        let ret: Result<(TcpStream, SocketAddr)> = async {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    return Err(anyhow!(
                        "err:Listener.accept => kind:{:?}, e:{}",
                        e.kind(),
                        e
                    ));
                }
            }
        }
        .await;
        let (stream, remote_addr) = ret?;
        tcp_util::set_stream(&stream, &self.config);
        Ok((
            Box::new(
                Connection::new(stream, remote_addr, self.config.clone(), self.sni.clone())
                    .map_err(|e| anyhow!("err:Connection::new => e:{}", e))?,
            ),
            false,
        ))
    }
}

pub struct Connection {
    stream: Option<TcpStream>,
    remote_addr: Option<SocketAddr>,
    config: Arc<Config>,
    sni: util::Sni,
}

impl Connection {
    pub fn new(
        stream: TcpStream,
        remote_addr: SocketAddr,
        config: Arc<Config>,
        sni: util::Sni,
    ) -> Result<Connection> {
        Ok(Connection {
            stream: Some(stream),
            remote_addr: Some(remote_addr),
            config,
            sni,
        })
    }
}

#[async_trait]
impl server::Connection for Connection {
    async fn stream(&mut self) -> Result<Option<(StreamFlow, ServerStreamInfo)>> {
        if self.stream.is_none() {
            return Ok(None);
        }
        let tcp_stream = self.stream.take().unwrap();
        let remote_addr = self.remote_addr.take().unwrap();
        let local_addr = tcp_stream.local_addr().unwrap();

        #[cfg(feature = "anyproxy-openssl")]
        {
            use openssl::ssl::Ssl;
            use tokio_openssl::SslStream;
            let tls_acceptor = self
                .sni
                .sni_openssl
                .tls_acceptor(b"\x02h2\x08http/1.1")
                .map_err(|e| anyhow!("err:sni_openssl.tls_acceptor => e:{}", e))?;
            let mut ssl =
                Ssl::new(tls_acceptor.context()).map_err(|e| anyhow!("err:Ssl::new => e:{}", e))?;
            ssl.set_accept_state();
            let mut ssl_stream = SslStream::new(ssl, tcp_stream)
                .map_err(|e| anyhow!("err:SslStream::new => e:{}", e))?;

            // std::pin::Pin::new(&mut ssl_stream)
            //     .do_handshake()
            //     .await
            //     .map_err(|e| anyhow!("err:Ssl::do_handshake => e:{}", e))?;

            let ret = std::pin::Pin::new(&mut ssl_stream).do_handshake().await;
            match ret {
                Err(e) => match e.io_error() {
                    Some(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                        return Ok(None);
                    }
                    Some(ref e) if e.kind() == std::io::ErrorKind::NotConnected => {
                        return Ok(None);
                    }
                    Some(ref e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                        return Ok(None);
                    }
                    _ => {
                        return Err(anyhow!("err:Ssl::do_handshake => e:{}", e));
                    }
                },
                _ => {}
            }

            let domain = ssl_stream
                .ssl()
                .servername(openssl::ssl::NameType::HOST_NAME);
            if domain.is_none() {
                return Err(anyhow!("err:openssl servername nil"));
            }
            let domain = domain.unwrap().to_string();

            let stream = Stream::new(StreamData::Openssl(ssl_stream));
            let mut stream = StreamFlow::new(stream, Some(vec!["shut down".to_string()]));
            let read_timeout =
                tokio::time::Duration::from_secs(self.config.tcp_recv_timeout as u64);
            let write_timeout =
                tokio::time::Duration::from_secs(self.config.tcp_send_timeout as u64);
            stream.set_config(read_timeout, write_timeout, None);
            let raw_fd = stream.raw_fd();
            Ok(Some((
                stream,
                ServerStreamInfo {
                    protocol7: Protocol7::Ssl,
                    remote_addr,
                    local_addr: Some(local_addr),
                    domain: Some(domain.into()),
                    is_tls: true,
                    raw_fd,
                },
            )))
        }

        #[cfg(feature = "anyproxy-rustls")]
        {
            let tls_acceptor = util::rustls::tls_acceptor(
                self.sni.sni_rustls.clone(),
                vec![b"http/1.1".to_vec(), b"h2".to_vec()],
            );
            let ssl_stream = tls_acceptor
                .accept(tcp_stream)
                .await
                .map_err(|e| anyhow!("err:tls_acceptor.accept => e:{}", e))?;
            let domain = {
                let (_, server_connection) = ssl_stream.get_ref();
                let domain = server_connection.sni_hostname();
                if domain.is_none() {
                    return Err(anyhow!(
                        "err:sni_hostname domain nil => local_addr:{}",
                        local_addr
                    ));
                }
                Some(domain.unwrap().into())
            };

            let stream = Stream::new(StreamData::S(ssl_stream));
            let mut stream = StreamFlow::new(stream, Some(vec!["shut down".to_string()]));
            let read_timeout =
                tokio::time::Duration::from_secs(self.config.tcp_recv_timeout as u64);
            let write_timeout =
                tokio::time::Duration::from_secs(self.config.tcp_send_timeout as u64);
            stream.set_config(read_timeout, write_timeout, None);
            let raw_fd = stream.raw_fd();
            Ok(Some((
                stream,
                ServerStreamInfo {
                    protocol7: Protocol7::Ssl,
                    remote_addr,
                    local_addr: Some(local_addr),
                    domain,
                    is_tls: true,
                    raw_fd,
                },
            )))
        }
    }
}
