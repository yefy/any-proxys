use crate::config::config_toml::TcpConfig as Config;
use anyhow::anyhow;
use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::SocketAddr;
use std::net::TcpListener as StdTcpListener;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;

pub fn bind(addr: &SocketAddr, _tcp_reuseport: bool) -> Result<StdTcpListener> {
    let addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "err:empty address"))?;

    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let sk = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| anyhow!("err:Socket::new => e:{}", e))?;
    let sk_addr = socket2::SockAddr::from(addr);
    #[cfg(unix)]
    {
        sk.set_reuse_port(_tcp_reuseport)?;
    }
    sk.set_nonblocking(true)
        .map_err(|e| anyhow!("err:sk.set_nonblocking => e:{}", e))?;
    sk.bind(&sk_addr)
        .map_err(|e| anyhow!("err:sk.bind => addr:{}, e:{}", addr.to_string(), e))?;
    sk.listen(1024)
        .map_err(|e| anyhow!("err:sk.listen => e:{}", e))?;
    Ok(sk.into())
}

pub fn set_stream(tcp_stream: &TcpStream, config: &Config) {
    let socket = socket2::SockRef::from(tcp_stream);

    if config.tcp_send_buffer > 0 {
        if let Err(e) = socket.set_send_buffer_size(config.tcp_send_buffer) {
            log::error!(
                "err:set_send_buffer_size => tcp_send_buffer:{}, e:{}",
                config.tcp_send_buffer,
                e
            );
        }
    }
    if config.tcp_recv_buffer > 0 {
        if let Err(e) = socket.set_recv_buffer_size(config.tcp_recv_buffer) {
            log::error!(
                "err:set_recv_buffer_size => tcp_recv_buffer:{}, e:{}",
                config.tcp_recv_buffer,
                e
            );
        }
    }

    if let Err(_e) = socket.set_nodelay(config.tcp_nodelay) {
        #[cfg(unix)]
        log::error!(
            "err:set_nodelay => tcp_nodelay:{}, e:{}",
            config.tcp_nodelay,
            _e
        );
    }
}
