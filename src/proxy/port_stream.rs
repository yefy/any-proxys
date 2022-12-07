use super::heartbeat_stream::HeartbeatStream;
use super::port_config;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::ebpf_sockhash;
use crate::io::buf_reader::BufReader;
use crate::stream::stream_flow;
use crate::Executors;
use crate::{protopack, Protocol7};
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use chrono::prelude::*;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

pub struct PortStream {
    executors: Executors,
    tunnel_publish: tunnel_server::Publish,
    tunnel2_publish: tunnel2_server::Publish,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    ssl_domain: Option<String>,
    shutdown_tx: broadcast::Sender<()>,
    port_config_listen: Rc<port_config::PortConfigListen>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
}

impl PortStream {
    pub fn new(
        executors: Executors,
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        ssl_domain: Option<String>,
        shutdown_tx: broadcast::Sender<()>,
        port_config_listen: Rc<port_config::PortConfigListen>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
    ) -> Result<PortStream> {
        Ok(PortStream {
            executors,
            tunnel_publish,
            tunnel2_publish,
            local_addr,
            remote_addr,
            ssl_domain,
            shutdown_tx,
            port_config_listen,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
        })
    }

    pub async fn start(
        &self,
        protocol7: Protocol7,
        mut stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let config_ctx = &self
            .port_config_listen
            .port_config_context
            .stream_config_context;
        let stream_info = StreamInfo::new(
            protocol7.to_string(),
            self.local_addr.clone(),
            self.remote_addr.clone(),
        );

        stream.set_stream_info(Some(stream_info.client_stream_info.clone()));
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let start_time = Instant::now();
        let stream_info = Rc::new(RefCell::new(stream_info));
        let ret: Option<Result<()>> = async {
            tokio::select! {
                biased;
                _ret = self.do_start(protocol7, stream_info.clone(), stream) => {
                    return Some(_ret);
                }
                _ = shutdown_rx.recv() => {
                    stream_info.borrow_mut().err_status = ErrStatus::ServerErr;
                    return None;
                }
                else => {
                    stream_info.borrow_mut().err_status = ErrStatus::ServerErr;
                    return None;
                }
            }
        }
        .await;

        let mut stream_info = stream_info.borrow_mut();
        let session_time = start_time.elapsed().as_secs_f32();
        stream_info.session_time = session_time;

        let is_close = StreamStream::stream_info_parse(&mut stream_info)
            .await
            .map_err(|e| anyhow!("err:stream_connect_parse => e:{}", e))?;
        if !stream_info.is_discard_flow {
            StreamStream::access_log(
                &config_ctx.access,
                &config_ctx.access_context,
                &config_ctx.stream_var,
                &mut stream_info,
            )
            .await
            .map_err(|e| anyhow!("err:StreamStream::access_log => e:{}", e))?;
        }

        if ret.is_some() {
            log::debug!("ret:{:?}", ret.as_ref().unwrap());
        }

        if is_close {
            return Ok(());
        }

        if ret.is_some() {
            let ret = ret.unwrap();
            let _ = ret?;
        }

        Ok(())
    }

    pub async fn do_start(
        &self,
        protocol7: Protocol7,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let (is_tunnel, buffer, client_stream) = TunnelStream::tunnel_stream(
            self.tunnel_publish.clone(),
            self.tunnel2_publish.clone(),
            self.local_addr.clone(),
            self.remote_addr.clone(),
            protocol7,
            &mut stream_info.borrow_mut(),
            client_stream,
        )
        .await
        .map_err(|e| anyhow!("err:tunnel_stream => e:{}", e))?;
        if is_tunnel {
            return Ok(());
        }
        let buffer = buffer.unwrap();
        let client_stream = client_stream.unwrap();
        let mut client_buf_reader = BufReader::new_from_buffer(&client_stream, buffer);

        let config_ctx = &self
            .port_config_listen
            .port_config_context
            .stream_config_context;
        let hello = {
            stream_info.borrow_mut().err_status = ErrStatus::ClientProtoErr;
            stream_info.borrow_mut().ssl_domain = self.ssl_domain.clone();
            stream_info.borrow_mut().local_domain = config_ctx.domain.clone();
            let hello = protopack::anyproxy::read_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
            match hello {
                Some(mut hello) => {
                    log::debug!("hello:{:?}", hello);
                    stream_info.borrow_mut().remote_domain = Some(hello.domain.clone());
                    if stream_info.borrow().local_domain.is_some() {
                        hello.domain = stream_info.borrow().local_domain.clone().unwrap();
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
                            "{}{}",
                            stream_info.borrow().remote_addr,
                            Local::now().timestamp()
                        ),
                        client_addr: stream_info.borrow().remote_addr.clone(),
                        domain: stream_info.borrow().local_domain.clone().unwrap(),
                    };

                    log::debug!("new hello:{:?}", hello);
                    hello
                }
            }
        };
        stream_info.borrow_mut().protocol_hello = Some(hello);
        StreamStream::work_time(
            "anyproxy::hello",
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

        HeartbeatStream::heartbeat_stream(
            &client_stream,
            &mut client_buf_reader,
            &mut stream_info.borrow_mut(),
        )
        .await?;

        let client_buffer = client_buf_reader.table_buffer();
        let (buf, pos, cap) = client_buffer;
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
