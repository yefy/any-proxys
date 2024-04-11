use super::heartbeat_stream::HeartbeatStream;
use super::proxy;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
use crate::protopack;
use crate::proxy::port_config::PortConfigListen;
use crate::proxy::util::run_plugin_handle_access;
use crate::stream::server::ServerStreamInfo;
use any_base::executor_local_spawn::ExecutorsLocal;
#[cfg(feature = "anyproxy-ebpf")]
use any_base::io::async_stream::Stream;
use any_base::module::module::Modules;
use any_base::stream_flow::StreamFlow;
use any_base::typ::Share;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use chrono::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct PortStream {
    ms: Modules,
    executors: ExecutorsLocal,
    server_stream_info: Arc<ServerStreamInfo>,
    tunnel_publish: Option<tunnel_server::Publish>,
    tunnel2_publish: Option<tunnel2_server::Publish>,
    port_config_listen: Arc<PortConfigListen>,
    client_stream: Option<StreamFlow>,
}

impl PortStream {
    pub fn new(
        ms: Modules,
        executors: ExecutorsLocal,
        server_stream_info: ServerStreamInfo,
        tunnel_publish: Option<tunnel_server::Publish>,
        tunnel2_publish: Option<tunnel2_server::Publish>,
        port_config_listen: Arc<PortConfigListen>,
        client_stream: StreamFlow,
    ) -> Result<PortStream> {
        Ok(PortStream {
            ms,
            executors,
            server_stream_info: Arc::new(server_stream_info),
            tunnel_publish,
            tunnel2_publish,
            port_config_listen,
            client_stream: Some(client_stream),
        })
    }

    pub async fn start(self) -> Result<()> {
        log::trace!("port stream start");
        let stream_info = {
            let session_id = {
                use crate::config::common_core;
                let common_core_conf = common_core::main_conf_mut(&self.ms).await;
                let session_id = common_core_conf.session_id.fetch_add(1, Ordering::Relaxed);
                session_id
            };
            let scc = self.port_config_listen.port_config_context.scc.get();
            let net_core_conf = scc.net_core_conf();
            StreamInfo::new(
                self.server_stream_info.clone(),
                net_core_conf.debug_is_open_stream_work_times,
                Some(self.executors.clone()),
                net_core_conf.debug_print_access_log_time,
                net_core_conf.debug_print_stream_flow_time,
                net_core_conf.stream_so_singer_time,
                net_core_conf.debug_is_open_print,
                session_id,
            )
        };
        let stream_info = Share::new(stream_info);
        let shutdown_thread_rx = self.executors.context.shutdown_thread_tx.subscribe();
        stream_start::do_start(self, stream_info, shutdown_thread_rx).await
    }
}

#[async_trait]
impl proxy::Stream for PortStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>) -> Result<()> {
        let mut _client_stream = self.client_stream.take().unwrap();
        _client_stream.set_stream_info(Some(stream_info.get().client_stream_flow_info.clone()));

        let scc = self.port_config_listen.port_config_context.scc.clone();
        stream_info.get_mut().scc = scc.clone();

        run_plugin_handle_access(scc.clone(), stream_info.clone()).await?;

        #[cfg(feature = "anyproxy-ebpf")]
        let _client_stream = if scc.get().net_core_conf().is_port_direct_ebpf
            && scc.get().net_core_conf().is_open_ebpf
            && _client_stream.raw_fd() > 0
        {
            stream_info.get_mut().add_work_time1("connect_and_ebpf");
            let ret =
                StreamStream::connect_and_ebpf(scc.clone(), stream_info.clone(), _client_stream)
                    .await?;
            if ret.is_none() {
                return Ok(());
            }
            ret.unwrap()
        } else {
            _client_stream
        };

        let client_buf_reader = any_base::io_rb::buf_reader::BufReader::new(_client_stream);
        stream_info.get_mut().add_work_time1("tunnel_stream");
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

        stream_info.get_mut().add_work_time1("heartbeat");
        let ret = HeartbeatStream::heartbeat_stream(
            client_buf_reader,
            stream_info,
            self.executors.clone(),
        )
        .await?;
        if ret.is_none() {
            return Ok(());
        }
        let (mut client_buf_reader, stream_info) = ret.unwrap();

        stream_info.get_mut().add_work_time1("hello");
        let hello = {
            stream_info.get_mut().err_status = ErrStatus::ClientProtoErr;
            //quic协议ssl_domain有值
            stream_info.get_mut().ssl_domain = self.server_stream_info.domain.clone();
            stream_info.get_mut().remote_domain = self.server_stream_info.domain.clone();
            //优先使用本地配置值
            stream_info.get_mut().local_domain = Some(scc.get().net_core_conf().domain.clone());
            let hello = protopack::anyproxy::read_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
            match hello {
                Some((mut hello, hello_pack_size)) => {
                    stream_info.get_mut().client_protocol_hello_size = hello_pack_size;
                    log::debug!("hello:{:?}", hello);
                    //优先使用hello的值
                    stream_info.get_mut().remote_domain = Some(hello.domain.clone());
                    if stream_info.get().local_domain.is_some() {
                        //优先使用本地配置值
                        hello.domain = stream_info.get().local_domain.clone().unwrap();
                    } else {
                        //优先使用本地配置值， 没有用hello的值
                        stream_info.get_mut().local_domain = Some(hello.domain.clone());
                    }
                    hello
                }
                None => {
                    let session_id = scc.get().common_core_conf().session_id.clone();
                    let stream_info = stream_info.get();
                    if stream_info.local_domain.is_none() {
                        return Err(anyhow!("err:not hello and local_domain.is_none()"));
                    }
                    let hello = protopack::anyproxy::AnyproxyHello {
                        version: protopack::anyproxy::ANYPROXY_VERSION.into(),
                        request_id: format!(
                            "{:?}_{}_{}_{}_{}",
                            stream_info.server_stream_info.local_addr,
                            stream_info.server_stream_info.remote_addr,
                            unsafe { libc::getpid() },
                            session_id.fetch_add(1, Ordering::Relaxed),
                            Local::now().timestamp_millis()
                        )
                        .into(),
                        client_addr: stream_info.server_stream_info.remote_addr.clone(),
                        domain: stream_info.local_domain.clone().unwrap(),
                    };

                    log::debug!("new hello:{:?}", hello);
                    hello
                }
            }
        };
        {
            let mut stream_info = stream_info.get_mut();
            stream_info.request_id = hello.request_id.clone();

            stream_info.protocol_hello.set(Arc::new(hello));

            log::debug!(
                "connect_and_stream request_id:{}, server_stream_info:{:?}",
                stream_info.request_id,
                self.server_stream_info
            );
        }

        stream_info.get_mut().add_work_time1("connect_and_stream");
        let (_client_stream, buf, pos, cap) = client_buf_reader.table_buffer_ext();
        let client_buffer = &buf[pos..cap];

        StreamStream::connect_and_stream(scc, stream_info, client_buffer, _client_stream).await
    }
}
