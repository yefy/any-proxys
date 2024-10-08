use crate::config::config_toml;
use crate::config::config_toml::QuicConfig as Config;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::util;
use anyhow::anyhow;
use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;

pub fn bind(
    addr: &SocketAddr,
    _reuseport: bool,
    recv_buffer_size: usize,
    send_buffer_size: usize,
) -> Result<std::net::UdpSocket> {
    let addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "err:empty address"))?;

    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let sk = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| anyhow!("err:Socket::new => e:{}", e))?;
    let sk_addr = socket2::SockAddr::from(addr);

    #[cfg(unix)]
    {
        sk.set_reuse_port(_reuseport)?;
    }
    if recv_buffer_size > 0 {
        if let Err(e) = sk.set_recv_buffer_size(recv_buffer_size) {
            log::error!("err:udp set_recv_buffer_size => e:{}", anyhow!("{}", e));
        }
    }

    if send_buffer_size > 0 {
        if let Err(e) = sk.set_send_buffer_size(send_buffer_size) {
            log::error!("err:udp set_send_buffer_size => e:{}", anyhow!("{}", e));
        }
    }

    sk.set_nonblocking(true)
        .map_err(|e| anyhow!("err:sk.set_nonblocking => e:{}", e))?;
    sk.bind(&sk_addr)
        .map_err(|e| anyhow!("err: sk.bind => addr:{}, e:{}", addr.to_string(), e))?;
    Ok(sk.into())
}

pub fn server_config(config: &Config, sni: Option<util::Sni>) -> Result<quinn::ServerConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    if config.quic_upstream_streams > 0 {
        transport_config
            .max_concurrent_bidi_streams(quinn::VarInt::from_u64(config.quic_upstream_streams)?);
    }

    // transport_config.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(
    //     30 * 1000 as u32,
    // ))));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));

    if !config.quic_default {
        let stream_rwnd: u32 = config.quic_max_stream_bandwidth * config.quic_rtt;
        log::debug!(target: "main", "config:{:?}", config);
        log::debug!(target: "main", "server_config stream_rwnd:{}", stream_rwnd);

        transport_config.receive_window(u32::MAX.into());
        if config.quic_send_window > 0 {
            transport_config.send_window((stream_rwnd * config.quic_send_window) as u64);
        } else {
            transport_config.send_window(u32::MAX.into());
        }
        transport_config.stream_receive_window(stream_rwnd.into()); // 110% of 1MiB.

        transport_config.datagram_receive_buffer_size(Some(stream_rwnd as usize));
        transport_config.datagram_send_buffer_size(config.quic_datagram_send_buffer_size);
    }

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_cert_resolver(sni.as_ref().unwrap().sni_rustls.clone());
    server_crypto.alpn_protocols = vec![config.quic_protocols.as_bytes().to_vec()];
    if config.quic_enable_keylog {
        server_crypto.key_log = Arc::new(rustls::KeyLogFile::new());
    }

    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
    server_config.transport = Arc::new(transport_config);
    //server_config.use_retry(true);

    Ok(server_config)
}

pub fn client_config(config: &Config) -> Result<quinn::ClientConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    if config.quic_upstream_streams > 0 {
        transport_config
            .max_concurrent_bidi_streams(quinn::VarInt::from_u64(config.quic_upstream_streams)?);
    }

    // transport_config.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(
    //     30 * 1000 as u32,
    // ))));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));

    if !config.quic_default {
        let stream_rwnd: u32 = config.quic_max_stream_bandwidth * config.quic_rtt;
        log::debug!(target: "main", "config:{:?}", config);
        log::debug!(target: "main", "client_config stream_rwnd:{}", stream_rwnd);

        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        transport_config.receive_window(u32::MAX.into());
        if config.quic_send_window > 0 {
            transport_config.send_window((stream_rwnd * config.quic_send_window) as u64);
        } else {
            transport_config.send_window(u32::MAX.into());
        }
        transport_config.stream_receive_window(stream_rwnd.into()); // 110% of 1MiB.
        transport_config.datagram_receive_buffer_size(Some(stream_rwnd as usize));
        transport_config.datagram_send_buffer_size(config.quic_datagram_send_buffer_size);
    };

    let mut client_crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(util::rustls::SkipServerVerification::new())
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![config.quic_protocols.as_bytes().to_vec()];
    if config.quic_enable_keylog {
        client_crypto.key_log = Arc::new(rustls::KeyLogFile::new());
    }

    let mut client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    client_config.transport_config(Arc::new(transport_config));
    Ok(client_config)
}

pub fn endpoint(
    config: &Config,
    mut reuseport: bool,
    addr: Option<SocketAddr>,
    is_ipv4: bool,
) -> Result<quinn::Endpoint> {
    //ipv6
    //let addr = &"[::]:0".parse()
    //    .map_err(|e| anyhow!("err:bind [::]:0 => e:{}", e))?;
    if reuseport {
        reuseport = false;
    }
    let addr = if addr.is_some() {
        addr.unwrap()
    } else {
        if is_ipv4 {
            "0.0.0.0:0"
                .parse()
                .map_err(|e| anyhow!("err:bind 0.0.0.0:0 => e:{}", e))?
        } else {
            "[::]:0"
                .parse()
                .map_err(|e| anyhow!("err:bind 0.0.0.0:0 => e:{}", e))?
        }
    };
    let client_config =
        client_config(config).map_err(|e| anyhow!("err:client_config => e:{}", e))?;
    let mut client_endpoint = if config.quic_default {
        //quinn::Endpoint::client(addr)?
        let udp_socket = bind(
            &addr,
            reuseport,
            config.quic_recv_buffer_size,
            config.quic_send_buffer_size,
        )
        .map_err(|e| anyhow!("err:bind => e:{}", e))?;
        quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            udp_socket,
            quinn::TokioRuntime,
        )
        .map_err(|e| anyhow!("err:Endpoint::new => e:{}", e))?
    } else {
        let udp_socket = bind(
            &addr,
            reuseport,
            config.quic_recv_buffer_size,
            config.quic_send_buffer_size,
        )
        .map_err(|e| anyhow!("err:bind => e:{}", e))?;
        quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            udp_socket,
            quinn::TokioRuntime,
        )?
    };

    client_endpoint.set_default_client_config(client_config);
    Ok(client_endpoint)
}

pub async fn listen(
    config: &Config,
    reuseport: bool,
    addr: &SocketAddr,
    sni: util::Sni,
    #[cfg(feature = "anyproxy-ebpf")] ebpf_tx: &Option<any_ebpf::AnyEbpfTx>,
) -> Result<quinn::Endpoint> {
    let udp_socket = bind(
        addr,
        reuseport,
        config.quic_recv_buffer_size,
        config.quic_send_buffer_size,
    )
    .map_err(|e| anyhow!("err:bind => e:{}", e))?;

    #[cfg(unix)]
    {
        #[cfg(feature = "anyproxy-ebpf")]
        {
            use std::os::unix::io::AsRawFd;

            let fd = udp_socket.as_raw_fd();

            let mut cookie: libc::c_ulonglong = 0;
            let mut optlen = std::mem::size_of::<libc::c_ulonglong>() as libc::socklen_t;

            unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_COOKIE,
                    &mut cookie as *mut _ as *mut _,
                    &mut optlen,
                )
            };

            log::debug!(target: "main", "cookie:{}, fd:{}", cookie, fd);
            if ebpf_tx.is_some() {
                ebpf_tx
                    .as_ref()
                    .unwrap()
                    .add_reuseport_data(cookie, fd)
                    .await?;
            }
        }
    }

    //let client_config =
    //   client_config(config).map_err(|e| anyhow!("err:client_config => e:{}", e))?;
    let server_config = server_config(config, Some(sni.clone()))
        .map_err(|e| anyhow!("err:server_config => e:{}", e))?;
    let endpoint = quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(server_config),
        udp_socket,
        quinn::TokioRuntime,
    )
    .map_err(|e| anyhow!("err:Endpoint::new => e:{}", e))?;

    //endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

pub fn sni(listens: &Vec<config_toml::Listen>) -> Result<util::Sni> {
    let sni_rustls = std::sync::Arc::new(sni_rustls(listens)?);
    #[cfg(feature = "anyproxy-openssl")]
    let sni_openssl = std::sync::Arc::new(sni_openssl(listens)?);
    Ok(util::Sni {
        sni_rustls,
        #[cfg(feature = "anyproxy-openssl")]
        sni_openssl,
    })
}

pub fn sni_rustls(
    listens: &Vec<config_toml::Listen>,
) -> Result<util::rustls::ResolvesServerCertUsingSNI> {
    let mut index = 0;
    let mut sni_contexts = Vec::with_capacity(50);
    let mut index_map = HashMap::new();

    for listen in listens.iter() {
        if listen.ssl.is_none() {
            log::error!("err:listen.ssl.is_none");
            continue;
        }
        let ssl = listen.ssl.clone();
        index = index + 1;
        index_map.insert(index, (ssl.as_ref().unwrap().ssl_domain.clone(), index));
        sni_contexts.push(util::SniContext { index, ssl });
    }

    let domain_index = util::domain_index::DomainIndex::new(&index_map)
        .map_err(|e| anyhow!("err:domain_index::DomainIndex => e:{}", e))?;
    let sni_rustls = util::rustls::ResolvesServerCertUsingSNI::new(&sni_contexts, domain_index)
        .map_err(|e| anyhow!("err:ResolvesServerCertUsingSNI => e:{}", e))?;

    Ok(sni_rustls)
}

#[cfg(feature = "anyproxy-openssl")]
pub fn sni_openssl(listens: &Vec<config_toml::Listen>) -> Result<util::openssl::OpensslSni> {
    let mut index = 0;
    let mut sni_contexts = Vec::with_capacity(50);
    let mut index_map = HashMap::new();

    for listen in listens.iter() {
        if listen.ssl.is_none() {
            log::error!("err:listen.ssl.is_none");
            continue;
        }
        let ssl = listen.ssl.clone();
        index = index + 1;
        index_map.insert(index, (ssl.as_ref().unwrap().ssl_domain.clone(), index));
        sni_contexts.push(util::SniContext { index, ssl });
    }

    let domain_index = util::domain_index::DomainIndex::new(&index_map)
        .map_err(|e| anyhow!("err:domain_index::DomainIndex => e:{}", e))?;
    let sni_openssl =
        util::openssl::OpensslSni::new(&sni_contexts, domain_index, util::openssl::PROTOCOL)
            .map_err(|e| anyhow!("err:OpensslSni => e:{}", e))?;

    Ok(sni_openssl)
}
