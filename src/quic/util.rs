use crate::config::config_toml;
use crate::config::config_toml::QuicConfig as Config;
use crate::util;
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
) -> anyhow::Result<std::net::UdpSocket> {
    let addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "err:empty address"))?;

    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let sk = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    let addr = socket2::SockAddr::from(addr);
    #[cfg(unix)]
    {
        sk.set_reuse_port(_reuseport)?;
    }
    if recv_buffer_size > 0 {
        if let Err(e) = sk.set_recv_buffer_size(recv_buffer_size) {
            log::error!(
                "err:udp set_recv_buffer_size => e:{}",
                anyhow::anyhow!("{}", e)
            );
        }
    }

    if send_buffer_size > 0 {
        if let Err(e) = sk.set_send_buffer_size(send_buffer_size) {
            log::error!(
                "err:udp set_send_buffer_size => e:{}",
                anyhow::anyhow!("{}", e)
            );
        }
    }

    sk.set_nonblocking(true)?;
    sk.bind(&addr)?;
    Ok(sk.into())
}

pub fn server_config(
    config: &Config,
    sni_rustls_map: Option<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>>,
) -> anyhow::Result<quinn::ServerConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    if config.quic_upstream_streams > 0 {
        transport_config
            .max_concurrent_bidi_streams(quinn::VarInt::from_u64(config.quic_upstream_streams)?);
    }

    // transport_config.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(
    //     30 as u32,
    // ))));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));

    if !config.quic_default {
        let stream_rwnd: u32 = config.quic_max_stream_bandwidth * config.quic_rtt;
        log::debug!("config:{:?}", config);
        log::debug!("server_config stream_rwnd:{}", stream_rwnd);

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
        .with_cert_resolver(sni_rustls_map.unwrap());
    server_crypto.alpn_protocols = vec![config.quic_protocols.as_bytes().to_vec()];
    if config.quic_enable_keylog {
        server_crypto.key_log = Arc::new(rustls::KeyLogFile::new());
    }

    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
    server_config.transport = Arc::new(transport_config);
    server_config.use_retry(true);

    Ok(server_config)
}

pub fn client_config(config: &Config) -> anyhow::Result<quinn::ClientConfig> {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    if config.quic_upstream_streams > 0 {
        transport_config
            .max_concurrent_bidi_streams(quinn::VarInt::from_u64(config.quic_upstream_streams)?);
    }

    // transport_config.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(
    //     30 as u32,
    // ))));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));

    if !config.quic_default {
        let stream_rwnd: u32 = config.quic_max_stream_bandwidth * config.quic_rtt;
        log::debug!("config:{:?}", config);
        log::debug!("client_config stream_rwnd:{}", stream_rwnd);

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
    client_config.transport = Arc::new(transport_config);
    Ok(client_config)
}

pub fn endpoint(config: &Config, reuseport: bool) -> anyhow::Result<quinn::Endpoint> {
    //ipv6
    //let addr = &"[::]:0".parse()
    //    .map_err(|e| anyhow::anyhow!("err:bind [::]:0 => e:{}", e))?;

    let addr = &"0.0.0.0:0"
        .parse()
        .map_err(|e| anyhow::anyhow!("err:bind 0.0.0.0:0 => e:{}", e))?;
    let client_config = client_config(config)?;
    let mut client_endpoint = if config.quic_default {
        //quinn::Endpoint::client(addr)?
        let udp_socket = bind(
            addr,
            reuseport,
            config.quic_recv_buffer_size,
            config.quic_send_buffer_size,
        )?;
        quinn::Endpoint::new(quinn::EndpointConfig::default(), None, udp_socket)?.0
    } else {
        let udp_socket = bind(
            addr,
            reuseport,
            config.quic_recv_buffer_size,
            config.quic_send_buffer_size,
        )?;
        quinn::Endpoint::new(quinn::EndpointConfig::default(), None, udp_socket)?.0
    };
    client_endpoint.set_default_client_config(client_config);
    Ok(client_endpoint)
}

pub async fn listen(
    config: &Config,
    reuseport: bool,
    addr: &SocketAddr,
    sni_rustls_map: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>,
) -> anyhow::Result<(quinn::Endpoint, quinn::Incoming)> {
    let udp_socket = bind(
        addr,
        reuseport,
        config.quic_recv_buffer_size,
        config.quic_send_buffer_size,
    )?;

    let client_config = client_config(config)?;
    let server_config = server_config(config, Some(sni_rustls_map.clone()))?;
    let (mut endpoint, incoming) = quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(server_config),
        udp_socket,
    )?;
    endpoint.set_default_client_config(client_config);
    Ok((endpoint, incoming))
}

pub fn sni(
    listens: &Vec<config_toml::Listen>,
) -> anyhow::Result<util::rustls::ResolvesServerCertUsingSNI> {
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

    let domain_index = util::domain_index::DomainIndex::new(&index_map)?;
    let sni_rustls = util::rustls::ResolvesServerCertUsingSNI::new(&sni_contexts, domain_index)?;

    Ok(sni_rustls)
}
