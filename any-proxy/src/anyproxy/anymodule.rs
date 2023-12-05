use crate::config::any_ebpf;
use crate::config::any_ebpf_core;
use crate::config::common;
use crate::config::common_core;
use crate::config::domain_core;
use crate::config::domain_listen_quic;
use crate::config::domain_listen_ssl;
use crate::config::domain_listen_tcp;
use crate::config::http;
use crate::config::http_access_log;
use crate::config::http_core;
use crate::config::http_core_plugin;
use crate::config::http_proxy_pass_quic;
use crate::config::http_proxy_pass_ssl;
use crate::config::http_proxy_pass_tcp;
use crate::config::http_proxy_pass_tunnel;
use crate::config::http_proxy_pass_tunnel2;
use crate::config::http_proxy_pass_upstream;
use crate::config::http_server_core;
use crate::config::http_server_core_plugin;
use crate::config::http_server_echo;
use crate::config::http_server_echo_websocket;
use crate::config::http_server_proxy;
use crate::config::http_server_proxy_websocket;
use crate::config::http_server_static;
use crate::config::http_server_static_websocket;
use crate::config::httplocal;
use crate::config::httpserver;
use crate::config::port_core;
use crate::config::port_listen_quic;
use crate::config::port_listen_ssl;
use crate::config::port_listen_tcp;
use crate::config::socket;
use crate::config::socket_quic;
use crate::config::socket_tcp;
use crate::config::stream_stream_cache;
use crate::config::stream_stream_memory;
use crate::config::stream_stream_tmp_file;
use crate::config::stream_stream_write;
use crate::config::tunnel;
use crate::config::tunnel2;
use crate::config::tunnel2_core;
use crate::config::tunnel_core;
use crate::config::upstream;
use crate::config::upstream_block;
use crate::config::upstream_core;
use crate::config::upstream_core_plugin;
use crate::config::upstream_proxy_pass_quic;
use crate::config::upstream_proxy_pass_ssl;
use crate::config::upstream_proxy_pass_tcp;
use crate::config::upstream_proxy_pass_tunnel;
use crate::config::upstream_proxy_pass_tunnel2;
use any_base::module::module;
use anyhow::Result;

pub fn add_modules() -> Result<()> {
    module::add_module(any_ebpf::module())?;
    module::add_module(any_ebpf_core::module())?;

    module::add_module(tunnel::module())?;
    module::add_module(tunnel_core::module())?;
    module::add_module(tunnel2::module())?;
    module::add_module(tunnel2_core::module())?;

    module::add_module(common::module())?;
    module::add_module(common_core::module())?;

    module::add_module(socket::module())?;
    module::add_module(socket_tcp::module())?;
    module::add_module(socket_quic::module())?;

    module::add_module(upstream::module())?;
    module::add_module(upstream_block::module())?;
    module::add_module(upstream_core::module())?;
    module::add_module(upstream_core_plugin::module())?;

    module::add_module(upstream_proxy_pass_tcp::module())?;
    module::add_module(upstream_proxy_pass_ssl::module())?;
    module::add_module(upstream_proxy_pass_quic::module())?;
    module::add_module(upstream_proxy_pass_tunnel::module())?;
    module::add_module(upstream_proxy_pass_tunnel2::module())?;

    module::add_module(http::module())?;
    module::add_module(httpserver::module())?;
    module::add_module(httplocal::module())?;
    module::add_module(http_core::module())?;
    module::add_module(http_core_plugin::module())?;
    module::add_module(http_server_core_plugin::module())?;
    module::add_module(http_server_core::module())?;

    module::add_module(domain_core::module())?;
    module::add_module(domain_listen_tcp::module())?;
    module::add_module(domain_listen_ssl::module())?;
    module::add_module(domain_listen_quic::module())?;

    module::add_module(port_core::module())?;
    module::add_module(port_listen_tcp::module())?;
    module::add_module(port_listen_ssl::module())?;
    module::add_module(port_listen_quic::module())?;

    module::add_module(http_server_echo::module())?;
    module::add_module(http_server_proxy::module())?;
    module::add_module(http_server_static::module())?;
    module::add_module(http_server_proxy_websocket::module())?;
    module::add_module(http_server_echo_websocket::module())?;
    module::add_module(http_server_static_websocket::module())?;

    module::add_module(http_proxy_pass_tcp::module())?;
    module::add_module(http_proxy_pass_ssl::module())?;
    module::add_module(http_proxy_pass_quic::module())?;
    module::add_module(http_proxy_pass_upstream::module())?;
    module::add_module(http_proxy_pass_tunnel::module())?;
    module::add_module(http_proxy_pass_tunnel2::module())?;

    module::add_module(http_access_log::module())?;

    module::add_module(stream_stream_cache::module())?;
    module::add_module(stream_stream_memory::module())?;
    module::add_module(stream_stream_tmp_file::module())?;
    module::add_module(stream_stream_write::module())?;

    module::parse_modules()?;

    Ok(())
}
