pub mod any_ebpf;
pub mod any_ebpf_core;
pub mod common;
pub mod common_core;
pub mod config_toml;
pub mod domain_core;
pub mod domain_listen_quic;
pub mod domain_listen_ssl;
pub mod domain_listen_tcp;
pub mod http_filter;
pub mod http_in;
pub mod net;
pub mod net_access;
pub mod net_access_ip;
pub mod net_access_log;
pub mod net_core;
pub mod net_core_plugin;
pub mod net_core_proxy;
pub mod net_core_wasm;
pub mod net_local;
pub mod net_local_core;
pub mod net_proxy_pass_quic;
pub mod net_proxy_pass_ssl;
pub mod net_proxy_pass_tcp;
pub mod net_proxy_pass_tunnel;
pub mod net_proxy_pass_tunnel2;
pub mod net_proxy_pass_upstream;
pub mod net_server;
pub mod net_server_core;
pub mod net_server_core_plugin;
pub mod net_server_http;
pub mod net_server_http_echo;
pub mod net_server_http_proxy;
pub mod net_server_http_purge;
pub mod net_server_http_static;
pub mod net_server_http_static_test;
pub mod net_server_stream_test;
pub mod net_server_websocket;
pub mod net_server_websocket_echo;
pub mod net_server_websocket_proxy;
pub mod net_server_websocket_static;
pub mod net_serverless;
pub mod port_core;
pub mod port_listen_quic;
pub mod port_listen_ssl;
pub mod port_listen_tcp;
pub mod socket;
pub mod socket_quic;
pub mod socket_tcp;
pub mod stream_stream_cache;
pub mod stream_stream_memory;
pub mod stream_stream_tmp_file;
pub mod stream_stream_write;
pub mod tunnel;
pub mod tunnel2;
pub mod tunnel2_core;
pub mod tunnel_core;
pub mod upstream;
pub mod upstream_block;
pub mod upstream_core;
pub mod upstream_core_plugin;
pub mod upstream_proxy_pass_quic;
pub mod upstream_proxy_pass_ssl;
pub mod upstream_proxy_pass_tcp;
pub mod upstream_proxy_pass_tunnel;
pub mod upstream_proxy_pass_tunnel2;

pub const MODULE_TYPE_NET: usize = 1 << 0;
pub const MODULE_TYPE_EBPF: usize = 1 << 1;
pub const MODULE_TYPE_UPSTREAM: usize = 1 << 2;
pub const MODULE_TYPE_COMMON: usize = 1 << 3;
pub const MODULE_TYPE_TUNNEL: usize = 1 << 4;
pub const MODULE_TYPE_TUNNEL2: usize = 1 << 5;
pub const MODULE_TYPE_SOCKET: usize = 1 << 6;

pub const CMD_CONF_TYPE_MAIN: usize = 1 << 0;
pub const CMD_CONF_TYPE_SERVER: usize = 1 << 1;
pub const CMD_CONF_TYPE_LOCAL: usize = 1 << 2;
