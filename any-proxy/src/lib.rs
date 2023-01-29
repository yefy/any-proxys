pub mod anyproxy;
pub mod config;
pub mod ebpf;
pub mod protopack;
pub mod proxy;
pub mod quic;
pub mod stream;
pub mod tcp;
pub mod tunnel;
pub mod tunnel2;
pub mod upstream;
pub mod util;

use any_base::io;
use any_tunnel::client;
use any_tunnel::server;
use any_tunnel2::client as client2;
use any_tunnel2::server as server2;
use any_tunnel2::Protocol4;
use anyhow::anyhow;
use anyhow::Result;

#[derive(Clone)]
pub struct Tunnels {
    client: client::Client,
    server: server::Server,
    client2: client2::Client,
    server2: server2::Server,
}

#[derive(Clone)]
pub struct TunnelClients {
    client: client::Client,
    client2: client2::Client,
}

#[derive(Clone, Eq, PartialEq)]
pub enum Protocol7 {
    Tcp,
    Quic,
    TunnelTcp,
    TunnelQuic,
    Tunnel2Tcp,
    Tunnel2Quic,
}

impl Protocol7 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol7::Tcp => "tcp".to_string(),
            Protocol7::Quic => "quic".to_string(),
            Protocol7::TunnelTcp => "tunnel_tcp".to_string(),
            Protocol7::TunnelQuic => "tunnel_quic".to_string(),
            Protocol7::Tunnel2Tcp => "tunnel2_tcp".to_string(),
            Protocol7::Tunnel2Quic => "tunnel2_quic".to_string(),
        }
    }

    pub fn from_string(name: String) -> Result<Protocol7> {
        match name.as_str() {
            "tcp" => Ok(Protocol7::Tcp),
            "quic" => Ok(Protocol7::Quic),
            "tunnel_tcp" => Ok(Protocol7::TunnelTcp),
            "tunnel_quic" => Ok(Protocol7::TunnelQuic),
            "tunnel2_tcp" => Ok(Protocol7::Tunnel2Tcp),
            "tunnel2_quic" => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:Protocol7 nil"));
            }
        }
    }

    pub fn to_tunnel_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::TunnelTcp),
            Protocol7::Quic => Ok(Protocol7::TunnelQuic),
            _ => {
                return Err(anyhow!("err:to_tunnel_protocol7"));
            }
        }
    }

    pub fn to_tunnel2_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::Tunnel2Tcp),
            Protocol7::Quic => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:to_tunnel2_protocol7"));
            }
        }
    }

    pub fn to_protocol4(&self) -> Protocol4 {
        match self {
            Protocol7::Tcp => Protocol4::TCP,
            Protocol7::Quic => Protocol4::UDP,
            Protocol7::TunnelTcp => Protocol4::TCP,
            Protocol7::TunnelQuic => Protocol4::UDP,
            Protocol7::Tunnel2Tcp => Protocol4::TCP,
            Protocol7::Tunnel2Quic => Protocol4::UDP,
        }
    }

    pub fn is_tunnel(&self) -> bool {
        match self {
            Protocol7::Tcp => false,
            Protocol7::Quic => false,
            Protocol7::TunnelTcp => true,
            Protocol7::TunnelQuic => true,
            Protocol7::Tunnel2Tcp => true,
            Protocol7::Tunnel2Quic => true,
        }
    }
}
