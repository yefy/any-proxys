use super::domain_config;
use super::heartbeat_stream::HeartbeatStream;
use super::proxy;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
use crate::protopack;
use crate::proxy::domain_context::DomainContext;
use crate::proxy::util as proxy_util;
use crate::proxy::ServerArg;
use crate::stream::server::ServerStreamInfo;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::module::module::Modules;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutexTokio, Share};
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct DomainStream {
    ms: Modules,
    executors: ExecutorsLocal,
    server_stream_info: Arc<ServerStreamInfo>,
    tunnel_publish: Option<tunnel_server::Publish>,
    tunnel2_publish: Option<tunnel2_server::Publish>,
    domain_config_listen: Arc<domain_config::DomainConfigListen>,
    domain_context: Arc<DomainContext>,
}

impl DomainStream {
    pub fn new(
        ms: Modules,
        executors: ExecutorsLocal,
        server_stream_info: ServerStreamInfo,
        tunnel_publish: Option<tunnel_server::Publish>,
        tunnel2_publish: Option<tunnel2_server::Publish>,
        domain_config_listen: Arc<domain_config::DomainConfigListen>,
        domain_context: Arc<DomainContext>,
    ) -> Result<DomainStream> {
        Ok(DomainStream {
            ms,
            executors,
            server_stream_info: Arc::new(server_stream_info),
            tunnel_publish,
            tunnel2_publish,
            domain_config_listen,
            domain_context,
        })
    }

    pub async fn start(self, mut stream: StreamFlow) -> Result<()> {
        log::trace!("domain stream start");
        use crate::config::http_core;
        let ms = self.ms.clone();
        let http_core_conf = http_core::main_conf_mut(&ms).await;

        let stream_info = StreamInfo::new(
            self.server_stream_info.clone(),
            http_core_conf.debug_is_open_stream_work_times,
            Some(self.executors.clone()),
        );
        let stream_info = Share::new(stream_info);
        stream.set_stream_info(stream_info.get().client_stream_flow_info.clone());
        let shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();

        stream_start::do_start(
            self,
            stream_info,
            stream,
            shutdown_thread_rx,
            http_core_conf.debug_print_access_log_time,
            http_core_conf.debug_print_stream_flow_time,
            http_core_conf.stream_so_singer_time,
        )
        .await
    }
}

#[async_trait]
impl proxy::Stream for DomainStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>, stream: StreamFlow) -> Result<()> {
        let client_buf_reader = any_base::io::buf_reader::BufReader::new(stream);
        stream_info.get_mut().add_work_time("tunnel_stream");

        let client_buf_reader = TunnelStream::tunnel_stream(
            self.tunnel_publish.clone(),
            self.tunnel2_publish.clone(),
            self.server_stream_info.clone(),
            client_buf_reader,
            stream_info.clone(),
            self.executors.clone(),
        )
        .await
        .map_err(|e| anyhow!("err:tunnel_stream => e:{}", e))?;
        if client_buf_reader.is_none() {
            return Ok(());
        }
        let client_buf_reader = client_buf_reader.unwrap();

        stream_info.get_mut().add_work_time("heartbeat");

        let ret = HeartbeatStream::heartbeat_stream(
            client_buf_reader,
            stream_info,
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

        let plugin_handle_protocol = self.domain_config_listen.plugin_handle_protocol.clone();
        if plugin_handle_protocol.is_some().await {
            return (plugin_handle_protocol.get().await)(arg, client_buf_reader).await;
        }

        let client_buf_reader = ArcMutexTokio::new(client_buf_reader);

        let client_buf_reader_hello = client_buf_reader.clone();
        let client_buf_reader_domain = client_buf_reader.clone();
        let server_stream_info = self.server_stream_info.clone();

        let scc = proxy_util::parse_proxy_domain(
            &arg,
            move || async move {
                let hello =
                    protopack::anyproxy::read_hello(&mut *client_buf_reader_hello.get_mut().await)
                        .await
                        .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
                Ok(hello)
            },
            move || async move {
                let domain = protopack::ssl_hello::read_domain(
                    &mut *client_buf_reader_domain.get_mut().await,
                )
                .await
                .map_err(|e| anyhow!("err:ssl_hello::read_domain => e:{}", e))?;
                let domain = match domain {
                    Some(domain) => domain,
                    None => {
                        if server_stream_info.domain.is_none() {
                            return Err(anyhow!("err:domain null"));
                        }
                        server_stream_info.domain.clone().unwrap()
                    }
                };
                Ok(domain)
            },
        )
        .await?;

        #[cfg(feature = "anyproxy-ebpf")]
        use crate::config::any_ebpf_core;
        #[cfg(feature = "anyproxy-ebpf")]
        let any_ebpf_core_conf = any_ebpf_core::main_conf(&self.ms).await;
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = any_ebpf_core_conf.ebpf();

        let client_buf_reader = unsafe { client_buf_reader.take().await };

        let (client_stream, buf, pos, cap) = client_buf_reader.table_buffer_ext();
        let client_buffer = &buf[pos..cap];
        StreamStream::connect_and_stream(
            scc,
            stream_info,
            client_buffer,
            client_stream,
            self.server_stream_info.local_addr.clone().unwrap(),
            self.server_stream_info.remote_addr,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
        )
        .await
    }
}
