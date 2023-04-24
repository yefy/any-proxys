use super::heartbeat_stream::HeartbeatStream;
use super::port_config;
use super::proxy;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::protopack;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use chrono::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct PortStream {
    executors: ExecutorsLocal,
    server_stream_info: Arc<ServerStreamInfo>,
    tunnel_publish: tunnel_server::Publish,
    tunnel2_publish: tunnel2_server::Publish,
    port_config_listen: Rc<port_config::PortConfigListen>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
}

impl PortStream {
    pub fn new(
        executors: ExecutorsLocal,
        server_stream_info: ServerStreamInfo,
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        port_config_listen: Rc<port_config::PortConfigListen>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<PortStream> {
        Ok(PortStream {
            executors,
            server_stream_info: Arc::new(server_stream_info),
            tunnel_publish,
            tunnel2_publish,
            port_config_listen,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
        })
    }

    pub async fn start(self, mut stream: stream_flow::StreamFlow) -> Result<()> {
        let (
            debug_print_access_log_time,
            debug_print_stream_flow_time,
            debug_is_open_stream_work_times,
            stream_so_singer_time,
        ) = {
            let stream_conf = &self
                .port_config_listen
                .port_config_context
                .stream_config_context
                .stream;

            (
                stream_conf.debug_print_access_log_time,
                stream_conf.debug_print_stream_flow_time,
                stream_conf.debug_is_open_stream_work_times,
                stream_conf.stream_so_singer_time,
            )
        };
        let stream_info = StreamInfo::new(
            self.server_stream_info.clone(),
            debug_is_open_stream_work_times,
        );
        let stream_info = Rc::new(RefCell::new(stream_info));

        stream.set_stream_info(Some(stream_info.borrow().client_stream_flow_info.clone()));
        let shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();

        stream_start::do_start(
            self,
            stream_info,
            stream,
            shutdown_thread_rx,
            debug_print_access_log_time,
            debug_print_stream_flow_time,
            stream_so_singer_time,
        )
        .await
    }
}

#[async_trait(?Send)]
impl proxy::Stream for PortStream {
    async fn do_start(
        &mut self,
        stream_info: Rc<RefCell<StreamInfo>>,
        stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let client_buf_reader = any_base::io::buf_reader::BufReader::new(stream);
        let stream_config_context = self
            .port_config_listen
            .port_config_context
            .stream_config_context
            .clone();
        stream_info.borrow_mut().stream_config_context = Some(stream_config_context.clone());
        stream_info.borrow_mut().debug_is_open_print =
            stream_config_context.fast_conf.debug_is_open_print;
        stream_info.borrow_mut().add_work_time("tunnel_stream");

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

        stream_info.borrow_mut().add_work_time("heartbeat");

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

        stream_info.borrow_mut().add_work_time("hello");

        let hello = {
            stream_info.borrow_mut().err_status = ErrStatus::ClientProtoErr;
            //quic协议ssl_domain有值
            stream_info.borrow_mut().ssl_domain = self.server_stream_info.domain.clone();
            stream_info.borrow_mut().remote_domain = self.server_stream_info.domain.clone();
            //优先使用本地配置值
            stream_info.borrow_mut().local_domain = stream_config_context.domain.clone();
            let hello = protopack::anyproxy::read_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
            match hello {
                Some((mut hello, hello_pack_size)) => {
                    stream_info.borrow_mut().client_protocol_hello_size = hello_pack_size;
                    log::debug!("hello:{:?}", hello);
                    //优先使用hello的值
                    stream_info.borrow_mut().remote_domain = Some(hello.domain.clone());
                    if stream_info.borrow().local_domain.is_some() {
                        //优先使用本地配置值
                        hello.domain = stream_info.borrow().local_domain.clone().unwrap();
                    } else {
                        //优先使用本地配置值， 没有用hello的值
                        stream_info.borrow_mut().local_domain = Some(hello.domain.clone());
                    }
                    hello
                }
                None => {
                    if stream_info.borrow().local_domain.is_none() {
                        return Err(anyhow!("err:not hello and local_domain.is_none()"));
                    }
                    let hello = protopack::anyproxy::AnyproxyHello {
                        version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                        request_id: format!(
                            "{:?}_{}_{}_{}_{}",
                            stream_info.borrow().server_stream_info.local_addr,
                            stream_info.borrow().server_stream_info.remote_addr,
                            unsafe { libc::getpid() },
                            self.session_id.fetch_add(1, Ordering::Relaxed),
                            Local::now().timestamp_millis()
                        ),
                        client_addr: stream_info.borrow().server_stream_info.remote_addr.clone(),
                        domain: stream_info.borrow().local_domain.clone().unwrap(),
                    };

                    log::debug!("new hello:{:?}", hello);
                    hello
                }
            }
        };

        stream_info.borrow_mut().request_id = hello.request_id.clone();
        {
            *stream_info.borrow().protocol_hello.lock().unwrap() = Some(Arc::new(hello));
        }

        log::debug!(
            "connect_and_stream request_id:{}, server_stream_info:{:?}",
            stream_info.borrow().request_id,
            self.server_stream_info
        );

        let (client_stream, buf, pos, cap) = client_buf_reader.table_buffer_ext();
        let client_buffer = &buf[pos..cap];
        StreamStream::connect_and_stream(
            stream_config_context,
            stream_info,
            client_buffer,
            client_stream,
            self.executors.thread_id.clone(),
            self.tmp_file_id.clone(),
            self.server_stream_info.local_addr.clone().unwrap(),
            self.server_stream_info.remote_addr,
            #[cfg(feature = "anyproxy-ebpf")]
            self.ebpf_add_sock_hash.clone(),
        )
        .await
    }
}
