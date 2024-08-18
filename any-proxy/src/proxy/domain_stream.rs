use super::domain_config;
use super::heartbeat_stream::HeartbeatStream;
use super::proxy;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
use crate::proxy::domain_context::DomainContext;
use crate::proxy::util as proxy_util;
use crate::proxy::util::{run_plugin_handle_access, run_plugin_handle_serverless};
use crate::proxy::ServerArg;
use crate::stream::server::ServerStreamInfo;
use crate::util::util::host_and_port;
use crate::{protopack, Protocol7};
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::module::module::Modules;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutexTokio, Share};
use any_base::util::ArcString;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct DomainStream {
    ms: Arc<Modules>,
    executors: ExecutorsLocal,
    server_stream_info: Arc<ServerStreamInfo>,
    tunnel_publish: Option<tunnel_server::Publish>,
    tunnel2_publish: Option<tunnel2_server::Publish>,
    domain_config_listen: Arc<domain_config::DomainConfigListen>,
    domain_context: Arc<DomainContext>,
    client_stream: Option<StreamFlow>,
}

impl DomainStream {
    pub fn new(
        ms: Arc<Modules>,
        executors: ExecutorsLocal,
        server_stream_info: ServerStreamInfo,
        tunnel_publish: Option<tunnel_server::Publish>,
        tunnel2_publish: Option<tunnel2_server::Publish>,
        domain_config_listen: Arc<domain_config::DomainConfigListen>,
        domain_context: Arc<DomainContext>,
        client_stream: StreamFlow,
    ) -> Result<DomainStream> {
        Ok(DomainStream {
            ms,
            executors,
            server_stream_info: Arc::new(server_stream_info),
            tunnel_publish,
            tunnel2_publish,
            domain_config_listen,
            domain_context,
            client_stream: Some(client_stream),
        })
    }

    pub async fn start(self) -> Result<()> {
        log::debug!(target: "ext3", "domain_stream.rs");
        use crate::config::net_core;
        let ms = self.ms.clone();
        let net_core_conf = net_core::main_conf_mut(&ms).await;
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&ms).await;
        let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);

        let mut stream_info = StreamInfo::new(
            self.server_stream_info.clone(),
            net_core_conf.debug_is_open_stream_work_times,
            Some(self.executors.clone()),
            net_core_conf.debug_print_access_log_time,
            net_core_conf.debug_print_stream_flow_time,
            net_core_conf.stream_so_singer_time,
            net_core_conf.debug_is_open_print,
            session_id,
        );

        let domain_config_context = self
            .domain_config_listen
            .domain_config_context_map
            .iter()
            .last();
        if domain_config_context.is_some() {
            let (_, domain_config_context) = domain_config_context.unwrap();
            stream_info.scc = Some(domain_config_context.scc.to_arc_scc(ms)).into();
        }
        let stream_info = Share::new(stream_info);
        let shutdown_thread_rx = self.executors.context.shutdown_thread_tx.subscribe();

        stream_start::do_start(self, &stream_info, shutdown_thread_rx).await
    }
}

#[async_trait]
impl proxy::Stream for DomainStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>) -> Result<()> {
        log::debug!(target: "ext3", "domain_stream.rs do_start");
        let mut stream = self.client_stream.take().unwrap();
        stream.set_stream_info(Some(stream_info.get().client_stream_flow_info.clone()));

        let client_buf_reader = any_base::io_rb::buf_reader::BufReader::new(stream);
        stream_info.get_mut().add_work_time1("tunnel_stream");
        log::debug!(target: "ext3", "domain_stream.rs tunnel_stream");
        let client_buf_reader = TunnelStream::tunnel_stream(
            self.tunnel_publish.clone(),
            self.tunnel2_publish.clone(),
            self.server_stream_info.clone(),
            client_buf_reader,
            &stream_info,
            self.executors.clone(),
        )
        .await
        .map_err(|e| anyhow!("err:tunnel_stream => e:{}", e))?;
        if client_buf_reader.is_none() {
            return Ok(());
        }
        let client_buf_reader = client_buf_reader.unwrap();

        log::debug!(target: "ext3", "domain_stream.rs heartbeat_stream");
        stream_info.get_mut().add_work_time1("heartbeat");
        let ret = HeartbeatStream::heartbeat_stream(
            client_buf_reader,
            stream_info.clone(),
            self.executors.clone(),
        )
        .await?;
        if ret.is_none() {
            return Ok(());
        }
        let (client_buf_reader, stream_info) = ret.unwrap();

        let arg = ServerArg {
            ms: self.ms.clone(),
            executors: self.executors.clone(),
            stream_info: stream_info.clone(),
            domain_config_listen: self.domain_config_listen.clone(),
            server_stream_info: self.server_stream_info.clone(),
            http_context: self.domain_context.http_context.clone(),
        };
        stream_info
            .get_mut()
            .add_work_time1("plugin_handle_protocol");
        log::debug!(target: "ext3", "domain_stream.rs plugin_handle_protocol");
        let plugin_handle_protocol = self.domain_config_listen.plugin_handle_protocol.clone();
        if plugin_handle_protocol.is_some().await {
            stream_info.get_mut().add_work_time1("plugin");
            return (plugin_handle_protocol.get().await)(arg, client_buf_reader.to_io_buf_reader())
                .await;
        }

        let client_buf_reader = ArcMutexTokio::new(client_buf_reader);

        let client_buf_reader_hello = client_buf_reader.clone();
        let client_buf_reader_domain = client_buf_reader.clone();
        let client_buf_reader_http_v1 = client_buf_reader.clone();
        let server_stream_info = self.server_stream_info.clone();

        log::debug!(target: "ext3", "domain_stream.rs parse_proxy_domain");
        stream_info.get_mut().add_work_time1("parse_proxy_domain");
        let stream_info_ = &stream_info;
        let scc = proxy_util::parse_proxy_domain(
            &arg,
            move || async move {
                let hello =
                    protopack::anyproxy::read_hello(&mut *client_buf_reader_hello.get_mut().await)
                        .await
                        .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
                Ok(hello)
            },
            move |domain_from_http_v1| async move {
                let domain = protopack::ssl_hello::read_domain(
                    &mut *client_buf_reader_domain.get_mut().await,
                )
                .await
                .map_err(|e| anyhow!("err:ssl_hello::read_domain => e:{}", e))?;
                let domain = match domain {
                    Some(domain) => {
                        stream_info_.get_mut().client_protocol7 = Some(Protocol7::Ssl);
                        ArcString::new(domain)
                    }
                    None => {
                        let domain = if domain_from_http_v1.is_open {
                            use crate::protopack::http_parse_v1;
                            let host = http_parse_v1::read_host(
                                &mut *client_buf_reader_http_v1.get_mut().await,
                                &domain_from_http_v1.check_methods,
                            )
                            .await?;
                            if host.is_some() {
                                let host = host.unwrap();
                                let (domain, _) = host_and_port(&host);
                                Some(domain.to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if domain.is_some() {
                            ArcString::new(domain.unwrap())
                        } else {
                            if server_stream_info.domain.is_none() {
                                return Err(anyhow!("err:domain null"));
                            }
                            server_stream_info.domain.clone().unwrap()
                        }
                    }
                };
                Ok(domain)
            },
        )
        .await?;
        let client_buf_reader = unsafe { client_buf_reader.take().await };

        log::debug!(target: "ext3", "domain_stream.rs run_plugin_handle_access");
        stream_info
            .get_mut()
            .add_work_time1("run_plugin_handle_access");
        if run_plugin_handle_access(&scc, &stream_info).await? {
            return Ok(());
        }

        log::debug!(target: "ext3", "domain_stream.rs run_plugin_handle_serverless");
        stream_info
            .get_mut()
            .add_work_time1("run_plugin_handle_serverless");
        let client_buf_reader =
            run_plugin_handle_serverless(&scc, &stream_info, client_buf_reader).await?;
        if client_buf_reader.is_none() {
            return Ok(());
        }
        let client_buf_reader = client_buf_reader.unwrap();

        let (client_stream, buf, pos, cap) = client_buf_reader.table_buffer_ext();
        let client_buffer = &buf[pos..cap];

        log::debug!(target: "ext3", "domain_stream.rs connect_and_stream");
        stream_info.get_mut().add_work_time1("connect_and_stream");
        StreamStream::connect_and_stream(&stream_info, client_buffer, client_stream).await
    }
}
