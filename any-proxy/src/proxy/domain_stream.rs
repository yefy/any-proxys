use super::domain_config;
use super::heartbeat_stream::HeartbeatStream;
use super::proxy;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::io::buf_reader::BufReader;
use crate::stream::stream_flow;
use crate::{protopack, Protocol7};
use any_base::executor_local_spawn::ExecutorsLocal;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use chrono::prelude::*;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct DomainStream {
    executors: ExecutorsLocal,
    tunnel_publish: tunnel_server::Publish,
    tunnel2_publish: tunnel2_server::Publish,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    ssl_domain: Option<String>,
    domain_config_listen: Rc<domain_config::DomainConfigListen>,
    domain_config_context: Option<Rc<domain_config::DomainConfigContext>>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
}

impl DomainStream {
    pub fn new(
        executors: ExecutorsLocal,
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        ssl_domain: Option<String>,
        domain_config_listen: Rc<domain_config::DomainConfigListen>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
    ) -> Result<DomainStream> {
        Ok(DomainStream {
            executors,
            tunnel_publish,
            tunnel2_publish,
            local_addr,
            remote_addr,
            ssl_domain,
            domain_config_listen,
            domain_config_context: None,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
        })
    }

    pub async fn start(
        self,
        protocol7: Protocol7,
        mut stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let (debug_print_access_log_time, debug_print_stream_flow_time) = {
            let stream_conf = &self.domain_config_listen.stream;

            (
                stream_conf.debug_print_access_log_time,
                stream_conf.debug_print_stream_flow_time,
            )
        };

        let stream_info = StreamInfo::new(
            protocol7.to_string(),
            self.local_addr.clone(),
            self.remote_addr.clone(),
            self.domain_config_listen
                .stream
                .debug_is_open_stream_work_times,
        );
        stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));
        let shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();

        stream_start::do_start(
            self,
            protocol7,
            stream_info,
            stream,
            shutdown_thread_rx,
            debug_print_access_log_time,
            debug_print_stream_flow_time,
        )
        .await
    }
}

#[async_trait(?Send)]
impl proxy::Stream for DomainStream {
    async fn do_start(
        &mut self,
        protocol7: Protocol7,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        stream_info.borrow_mut().add_work_time("tunnel_stream");

        let client_buf_reader = BufReader::new(client_stream);
        let (is_tunnel, client_buf_reader) = TunnelStream::tunnel_stream(
            self.tunnel_publish.clone(),
            self.tunnel2_publish.clone(),
            self.local_addr.clone(),
            self.remote_addr.clone(),
            protocol7,
            client_buf_reader,
            stream_info.clone(),
        )
        .await
        .map_err(|e| anyhow!("err:tunnel_stream => e:{}", e))?;
        if is_tunnel {
            return Ok(());
        }
        let client_buf_reader = client_buf_reader.unwrap();

        stream_info.borrow_mut().add_work_time("heartbeat");

        let (mut client_buf_reader, stream_info) =
            HeartbeatStream::heartbeat_stream(client_buf_reader, stream_info).await?;

        stream_info.borrow_mut().add_work_time("hello");

        let hello = {
            stream_info.borrow_mut().err_status = ErrStatus::ClientProtoErr;
            let hello = protopack::anyproxy::read_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
            match hello {
                Some(hello) => {
                    if self.ssl_domain.is_some() {
                        stream_info.borrow_mut().ssl_domain = self.ssl_domain.clone();
                    } else {
                        stream_info.borrow_mut().ssl_domain = Some(hello.domain.clone());
                    }
                    stream_info.borrow_mut().remote_domain = Some(hello.domain.clone());
                    stream_info.borrow_mut().local_domain = Some(hello.domain.clone());
                    hello
                }
                None => {
                    let domain = protopack::ssl_hello::read_domain(&mut client_buf_reader)
                        .await
                        .map_err(|e| anyhow!("err:ssl_hello::read_domain => e:{}", e))?;
                    let domain = match domain {
                        Some(domain) => domain,
                        None => {
                            if self.ssl_domain.is_none() {
                                return Err(anyhow!("err:domain null"));
                            }
                            self.ssl_domain.clone().unwrap()
                        }
                    };

                    if self.ssl_domain.is_some() {
                        stream_info.borrow_mut().ssl_domain = self.ssl_domain.clone();
                    } else {
                        stream_info.borrow_mut().ssl_domain = Some(domain.clone());
                    }
                    stream_info.borrow_mut().ssl_domain = Some(domain.clone());
                    stream_info.borrow_mut().remote_domain = Some(domain.clone());
                    stream_info.borrow_mut().local_domain = Some(domain.clone());

                    protopack::anyproxy::AnyproxyHello {
                        version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                        request_id: format!(
                            "{}_{}",
                            self.session_id.fetch_add(1, Ordering::Relaxed),
                            Local::now().timestamp()
                        ),
                        client_addr: stream_info.borrow().remote_addr.clone(),
                        domain,
                    }
                }
            }
        };
        {
            stream_info.borrow_mut().request_id = hello.request_id.clone();
            *stream_info.borrow().protocol_hello.lock().unwrap() = Some(hello);
        }

        let domain_index = {
            self.domain_config_listen
                .domain_index
                .index(
                    &stream_info
                        .borrow()
                        .protocol_hello
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .domain,
                )
                .map_err(|e| anyhow!("err:domain_index.index => e:{}", e))?
        };
        let domain_config_context = self
            .domain_config_listen
            .domain_config_context_map
            .get(&domain_index)
            .cloned()
            .ok_or(anyhow!(
                "err:domain_index nil => domain_index:{}",
                domain_index
            ))?;

        self.domain_config_context = Some(domain_config_context);
        let config_ctx = self
            .domain_config_context
            .as_ref()
            .unwrap()
            .stream_config_context
            .clone();

        stream_info.borrow_mut().stream_config_context = Some(config_ctx.clone());
        stream_info.borrow_mut().debug_is_open_print = config_ctx.fast_conf.debug_is_open_print;
        stream_info.borrow_mut().debug_is_open_stream_work_times =
            config_ctx.stream.debug_is_open_stream_work_times;

        let (client_stream, buf, pos, cap) = client_buf_reader.table_buffer_ext();
        let client_buffer = &buf[pos..cap];
        StreamStream::connect_and_stream(
            config_ctx.clone(),
            stream_info,
            client_buffer,
            client_stream,
            self.executors.thread_id.clone(),
            self.tmp_file_id.clone(),
            &self.local_addr,
            &self.remote_addr,
            #[cfg(feature = "anyproxy-ebpf")]
            self.ebpf_add_sock_hash.clone(),
        )
        .await
    }
}
