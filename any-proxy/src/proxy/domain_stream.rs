use super::domain_config;
use super::heartbeat_stream::HeartbeatStream;
use super::http_proxy::http_server;
use super::proxy;
use super::stream_info::StreamInfo;
use super::stream_start;
use super::stream_stream::StreamStream;
use super::tunnel_stream::TunnelStream;
use crate::config::config_toml::ServerConfigType;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::protopack;
use crate::proxy::domain_context::DomainContext;
use crate::proxy::util as proxy_util;
use crate::proxy::websocket_proxy::websocket_server;
use crate::proxy::ServerArg;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub struct DomainStream {
    executors: ExecutorsLocal,
    server_stream_info: Arc<ServerStreamInfo>,
    tunnel_publish: tunnel_server::Publish,
    tunnel2_publish: tunnel2_server::Publish,
    domain_config_listen: Rc<domain_config::DomainConfigListen>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
    domain_context: Rc<DomainContext>,
}

impl DomainStream {
    pub fn new(
        executors: ExecutorsLocal,
        server_stream_info: ServerStreamInfo,
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        domain_config_listen: Rc<domain_config::DomainConfigListen>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
        domain_context: Rc<DomainContext>,
    ) -> Result<DomainStream> {
        Ok(DomainStream {
            executors,
            server_stream_info: Arc::new(server_stream_info),
            tunnel_publish,
            tunnel2_publish,
            domain_config_listen,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
            domain_context,
        })
    }

    pub async fn start(self, mut stream: stream_flow::StreamFlow) -> Result<()> {
        let (debug_print_access_log_time, debug_print_stream_flow_time, stream_so_singer_time) = {
            let stream_conf = &self.domain_config_listen.stream;

            (
                stream_conf.debug_print_access_log_time,
                stream_conf.debug_print_stream_flow_time,
                stream_conf.stream_so_singer_time,
            )
        };

        let stream_info = StreamInfo::new(
            self.server_stream_info.clone(),
            self.domain_config_listen
                .stream
                .debug_is_open_stream_work_times,
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
impl proxy::Stream for DomainStream {
    async fn do_start(
        &mut self,
        stream_info: Rc<RefCell<StreamInfo>>,
        stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let client_buf_reader = any_base::io::buf_reader::BufReader::new(stream);
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
        let (client_buf_reader, stream_info) = ret.unwrap();

        let arg = ServerArg {
            executors: self.executors.clone(),
            stream_info: stream_info.clone(),
            domain_config_listen: self.domain_config_listen.clone(),
            server_stream_info: self.server_stream_info.clone(),
            session_id: self.session_id.clone(),
            http_context: self.domain_context.http_context.clone(),
            tmp_file_id: self.tmp_file_id.clone(),
        };

        match self.domain_config_listen.server_type {
            ServerConfigType::Nil => {
                //next
            }
            ServerConfigType::HttpServer => {
                let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                    any_base::io::buf_writer::BufWriter::new(client_buf_reader),
                );
                http_server::http_connection(arg, client_buf_stream).await?;
                return Ok(());
            }
            ServerConfigType::WebsocketServer => {
                let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                    any_base::io::buf_writer::BufWriter::new(client_buf_reader),
                );
                return websocket_server::WebsocketServer::new(arg)
                    .run(client_buf_stream)
                    .await;
            }
        }

        let client_buf_reader = Arc::new(tokio::sync::Mutex::new(Some(client_buf_reader)));

        let client_buf_reader_hello = client_buf_reader.clone();
        let client_buf_reader_domain = client_buf_reader.clone();
        let server_stream_info = self.server_stream_info.clone();

        let stream_config_context = proxy_util::parse_proxy_domain(
            &arg,
            move || async move {
                let hello = protopack::anyproxy::read_hello(
                    &mut client_buf_reader_hello.lock().await.as_mut().unwrap(),
                )
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
                Ok(hello)
            },
            move || async move {
                let domain = protopack::ssl_hello::read_domain(
                    &mut client_buf_reader_domain.lock().await.as_mut().unwrap(),
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

        let client_buf_reader = client_buf_reader.lock().await.take().unwrap();

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
