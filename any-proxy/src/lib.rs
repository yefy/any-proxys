pub mod anyproxy;
pub mod config;
pub mod ebpf;
pub mod protopack;
pub mod proxy;
pub mod quic;
pub mod ssl;
pub mod stream;
pub mod tcp;
pub mod tunnel;
pub mod tunnel2;
pub mod upstream;
pub mod util;

use any_tunnel2::Protocol4;
use anyhow::anyhow;
use anyhow::Result;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Protocol7 {
    Tcp,
    Ssl,
    Quic,
    TunnelTcp,
    TunnelSsl,
    TunnelQuic,
    Tunnel2Tcp,
    Tunnel2Ssl,
    Tunnel2Quic,
}

impl Protocol7 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol7::Tcp => "tcp".to_string(),
            Protocol7::Ssl => "ssl".to_string(),
            Protocol7::Quic => "quic".to_string(),
            Protocol7::TunnelTcp => "tunnel_tcp".to_string(),
            Protocol7::TunnelSsl => "tunnel_ssl".to_string(),
            Protocol7::TunnelQuic => "tunnel_quic".to_string(),
            Protocol7::Tunnel2Tcp => "tunnel2_tcp".to_string(),
            Protocol7::Tunnel2Ssl => "tunnel2_ssl".to_string(),
            Protocol7::Tunnel2Quic => "tunnel2_quic".to_string(),
        }
    }

    pub fn from_string(name: &str) -> Result<Protocol7> {
        match name {
            "tcp" => Ok(Protocol7::Tcp),
            "ssl" => Ok(Protocol7::Ssl),
            "quic" => Ok(Protocol7::Quic),
            "tunnel_tcp" => Ok(Protocol7::TunnelTcp),
            "tunnel_ssl" => Ok(Protocol7::TunnelSsl),
            "tunnel_quic" => Ok(Protocol7::TunnelQuic),
            "tunnel2_tcp" => Ok(Protocol7::Tunnel2Tcp),
            "tunnel2_ssl" => Ok(Protocol7::Tunnel2Ssl),
            "tunnel2_quic" => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:Protocol7 nil"));
            }
        }
    }

    pub fn to_tunnel_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::TunnelTcp),
            Protocol7::Ssl => Ok(Protocol7::TunnelSsl),
            Protocol7::Quic => Ok(Protocol7::TunnelQuic),
            _ => {
                return Err(anyhow!("err:to_tunnel_protocol7"));
            }
        }
    }

    pub fn to_tunnel2_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::Tunnel2Tcp),
            Protocol7::Ssl => Ok(Protocol7::Tunnel2Ssl),
            Protocol7::Quic => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:to_tunnel2_protocol7"));
            }
        }
    }

    pub fn to_protocol4(&self) -> Protocol4 {
        match self {
            Protocol7::Tcp => Protocol4::TCP,
            Protocol7::Ssl => Protocol4::TCP,
            Protocol7::Quic => Protocol4::UDP,
            Protocol7::TunnelTcp => Protocol4::TCP,
            Protocol7::TunnelSsl => Protocol4::TCP,
            Protocol7::TunnelQuic => Protocol4::UDP,
            Protocol7::Tunnel2Tcp => Protocol4::TCP,
            Protocol7::Tunnel2Ssl => Protocol4::TCP,
            Protocol7::Tunnel2Quic => Protocol4::UDP,
        }
    }

    pub fn is_tunnel(&self) -> bool {
        match self {
            Protocol7::Tcp => false,
            Protocol7::Ssl => false,
            Protocol7::Quic => false,
            Protocol7::TunnelTcp => true,
            Protocol7::TunnelSsl => true,
            Protocol7::TunnelQuic => true,
            Protocol7::Tunnel2Tcp => true,
            Protocol7::Tunnel2Ssl => true,
            Protocol7::Tunnel2Quic => true,
        }
    }
}

pub enum Protocol77 {
    Http,
    Http2,
    WebSocket,
    Https,
    Https2,
    WebSockets,
}

impl Protocol77 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol77::Http => "http".to_string(),
            Protocol77::Http2 => "http2".to_string(),
            Protocol77::WebSocket => "webSocket".to_string(),
            Protocol77::Https => "https".to_string(),
            Protocol77::Https2 => "https2".to_string(),
            Protocol77::WebSockets => "webSockets".to_string(),
        }
    }

    pub fn from_string(name: &str) -> Result<Protocol77> {
        match name {
            "http" => Ok(Protocol77::Http),
            "http2" => Ok(Protocol77::Http2),
            "webSocket" => Ok(Protocol77::WebSocket),
            "https" => Ok(Protocol77::Https),
            "https2" => Ok(Protocol77::Https2),
            "webSockets" => Ok(Protocol77::WebSockets),
            _ => {
                return Err(anyhow!("err:Protocol77 nil"));
            }
        }
    }
}
