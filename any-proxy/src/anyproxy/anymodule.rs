use crate::config::any_ebpf;
use crate::config::any_ebpf_core;
use crate::config::common;
use crate::config::common_core;
use crate::config::domain_core;
use crate::config::domain_listen_quic;
use crate::config::domain_listen_ssl;
use crate::config::domain_listen_tcp;
use crate::config::net;
use crate::config::net_core;
use crate::config::net_core_plugin;
use crate::config::net_local;
use crate::config::net_local_core;
use crate::config::net_proxy_pass_quic;
use crate::config::net_proxy_pass_ssl;
use crate::config::net_proxy_pass_tcp;
use crate::config::net_proxy_pass_tunnel;
use crate::config::net_proxy_pass_tunnel2;
use crate::config::net_proxy_pass_upstream;
use crate::config::net_server;
use crate::config::net_server_core;
use crate::config::net_server_core_plugin;
use crate::config::net_server_echo_http;
use crate::config::net_server_echo_websocket;
use crate::config::net_server_proxy_http;
use crate::config::net_server_proxy_websocket;
use crate::config::net_server_static_http;
use crate::config::net_server_static_websocket;
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

    module::add_module(net::module())?;
    module::add_module(net_server::module())?;
    module::add_module(net_local::module())?;
    module::add_module(net_local_core::module())?;
    module::add_module(net_core::module())?;
    module::add_module(net_core_plugin::module())?;
    module::add_module(net_server_core_plugin::module())?;
    module::add_module(net_server_core::module())?;

    module::add_module(domain_core::module())?;
    module::add_module(domain_listen_tcp::module())?;
    module::add_module(domain_listen_ssl::module())?;
    module::add_module(domain_listen_quic::module())?;

    module::add_module(port_core::module())?;
    module::add_module(port_listen_tcp::module())?;
    module::add_module(port_listen_ssl::module())?;
    module::add_module(port_listen_quic::module())?;

    module::add_module(net_server_echo_http::module())?;
    module::add_module(net_server_proxy_http::module())?;
    module::add_module(net_server_static_http::module())?;
    module::add_module(net_server_proxy_websocket::module())?;
    module::add_module(net_server_echo_websocket::module())?;
    module::add_module(net_server_static_websocket::module())?;

    module::add_module(net_proxy_pass_tcp::module())?;
    module::add_module(net_proxy_pass_ssl::module())?;
    module::add_module(net_proxy_pass_quic::module())?;
    module::add_module(net_proxy_pass_upstream::module())?;
    module::add_module(net_proxy_pass_tunnel::module())?;
    module::add_module(net_proxy_pass_tunnel2::module())?;

    use crate::config::net_access;
    module::add_module(net_access::module())?;

    use crate::config::net_serverless;
    module::add_module(net_serverless::module())?;

    use crate::config::net_access_log;
    module::add_module(net_access_log::module())?;

    module::add_module(stream_stream_cache::module())?;
    module::add_module(stream_stream_memory::module())?;
    module::add_module(stream_stream_tmp_file::module())?;
    module::add_module(stream_stream_write::module())?;

    use crate::config::net_server_stream_test;
    module::add_module(net_server_stream_test::module())?;
    use crate::config::net_server_static_http_test;
    module::add_module(net_server_static_http_test::module())?;

    use crate::config::net_core_proxy;
    module::add_module(net_core_proxy::module())?;

    use crate::config::net_core_wasm;
    module::add_module(net_core_wasm::module())?;

    use crate::config::http_in::http_in_header_start;
    module::add_module(http_in_header_start::module())?;
    use crate::config::http_in::http_in_headers;
    module::add_module(http_in_headers::module())?;

    use crate::config::http_filter::http_filter_header_start;
    module::add_module(http_filter_header_start::module())?;
    use crate::config::http_filter::http_filter_headers_pre;
    module::add_module(http_filter_headers_pre::module())?;
    use crate::config::http_filter::http_filter_header_parse;
    module::add_module(http_filter_header_parse::module())?;
    use crate::config::http_filter::http_filter_header_not_modified;
    module::add_module(http_filter_header_not_modified::module())?;
    use crate::config::http_filter::http_filter_header_range;
    module::add_module(http_filter_header_range::module())?;
    use crate::config::http_filter::http_filter_headers;
    module::add_module(http_filter_headers::module())?;
    use crate::config::http_filter::http_filter_header;
    module::add_module(http_filter_header::module())?;

    use crate::config::http_filter::http_filter_body_start;
    module::add_module(http_filter_body_start::module())?;
    use crate::config::http_filter::http_filter_body_range;
    module::add_module(http_filter_body_range::module())?;

    Ok(())
}
