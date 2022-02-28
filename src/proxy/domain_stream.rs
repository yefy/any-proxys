use super::domain_config;
use super::stream_connect;
use super::stream_stream::StreamStream;
use crate::io::buf_reader::BufReader;
use crate::io::buf_stream::BufStream;
use crate::io::buf_writer::BufWriter;
use crate::protopack;
use crate::stream::stream_flow;
use crate::Protocol7;
use any_tunnel::protopack::TUNNEL_VERSION;
use any_tunnel::server as tunnel_server;
use any_tunnel2::protopack::TUNNEL_VERSION as TUNNEL2_VERSION;
use any_tunnel2::server as tunnel2_server;
use chrono::prelude::*;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;

pub struct DomainStream {
    tunnel_publish: tunnel_server::Publish,
    tunnel2_publish: tunnel2_server::Publish,
    _executor: async_executors::TokioCt,
    _group_version: i32,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    ssl_domain: Option<String>,
    shutdown_tx: broadcast::Sender<()>,
    domain_config_listen: Rc<domain_config::DomainConfigListen>,
    domain_config_context: Option<Rc<domain_config::DomainConfigContext>>,
}

impl DomainStream {
    pub fn new(
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        _executor: async_executors::TokioCt,
        _group_version: i32,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        ssl_domain: Option<String>,
        shutdown_tx: broadcast::Sender<()>,
        domain_config_listen: Rc<domain_config::DomainConfigListen>,
    ) -> anyhow::Result<DomainStream> {
        Ok(DomainStream {
            tunnel_publish,
            tunnel2_publish,
            _executor,
            _group_version,
            local_addr,
            remote_addr,
            ssl_domain,
            shutdown_tx,
            domain_config_listen,
            domain_config_context: None,
        })
    }

    pub async fn start(
        &mut self,
        protocol7: Protocol7,
        mut stream: stream_flow::StreamFlow,
    ) -> anyhow::Result<()> {
        let mut stream_connect = stream_connect::StreamConnect::new(
            protocol7.to_string(),
            self.local_addr.clone(),
            self.remote_addr.clone(),
        );
        let read_timeout =
            tokio::time::Duration::from_secs(self.domain_config_listen.stream.stream_recv_timeout);
        let write_timeout =
            tokio::time::Duration::from_secs(self.domain_config_listen.stream.stream_send_timeout);
        stream.set_config(
            read_timeout,
            write_timeout,
            true,
            Some(stream_connect.client_stream_info.clone()),
        );
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let start_time = Instant::now();
        let mut ret: Option<anyhow::Result<()>> = None;
        loop {
            tokio::select! {
                biased;
                _ret = self.do_start(protocol7, &mut stream_connect, stream) => {
                    ret = Some(_ret);
                    break;
                }
                _ = shutdown_rx.recv() => {
                    stream_connect.err_status = stream_connect::ErrStatus::ServerErr;
                    break;
                }
                else => {
                    stream_connect.err_status = stream_connect::ErrStatus::ServerErr;
                    break;
                }
            }
        }
        let session_time = start_time.elapsed().as_secs_f32();
        stream_connect.session_time = session_time;

        let is_close = StreamStream::stream_connect_parse(&mut stream_connect).await?;

        if !stream_connect.is_discard_flow {
            if self.domain_config_context.is_some() {
                let domain_config_context = self.domain_config_context.as_ref().unwrap();
                StreamStream::access_log(
                    &domain_config_context.access,
                    &domain_config_context.access_context,
                    &domain_config_context.stream_var,
                    &mut stream_connect,
                )
                .await?;
            }
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
        &mut self,
        protocol7: Protocol7,
        stream_connect: &mut stream_connect::StreamConnect,
        mut client_stream: stream_flow::StreamFlow,
    ) -> anyhow::Result<()> {
        log::debug!("server protocol7:{}", protocol7.to_string());
        let mut client_buf_reader = BufReader::new(&client_stream);
        if !protocol7.is_tunnel() {
            {
                let tunnel_hello =
                    protopack::anyproxy::read_tunnel_hello(&mut client_buf_reader).await?;
                if tunnel_hello.is_some() {
                    let tunnel_hello = tunnel_hello.unwrap();
                    if &tunnel_hello.version == TUNNEL_VERSION {
                        stream_connect.is_discard_flow = true;
                        let buffer = client_buf_reader.table_buffer();
                        let buf_reader = BufReader::new_from_buffer(client_stream, buffer);
                        let buf_writer = BufWriter::new(buf_reader);
                        let buf_stream = BufStream::from(buf_writer);
                        self.tunnel_publish
                            .push_peer_stream(
                                buf_stream,
                                self.local_addr.clone(),
                                self.remote_addr.clone(),
                            )
                            .await?;
                        return Ok(());
                    }
                }
            }

            {
                let tunnel_hello =
                    protopack::anyproxy::read_tunnel2_hello(&mut client_buf_reader).await?;
                if tunnel_hello.is_some() {
                    let tunnel_hello = tunnel_hello.unwrap();
                    if &tunnel_hello.version == TUNNEL2_VERSION {
                        stream_connect.is_discard_flow = true;
                        let buffer = client_buf_reader.table_buffer();
                        let buf_reader = BufReader::new_from_buffer(client_stream, buffer);
                        let buf_writer = BufWriter::new(buf_reader);
                        let buf_stream = BufStream::from(buf_writer);
                        self.tunnel2_publish
                            .push_peer_stream(
                                buf_stream,
                                self.local_addr.clone(),
                                self.remote_addr.clone(),
                            )
                            .await?;
                        return Ok(());
                    }
                }
            }
        }

        let hello = {
            stream_connect.err_status = stream_connect::ErrStatus::ClientProtoErr;
            let hello = protopack::anyproxy::read_hello(&mut client_buf_reader).await?;
            match hello {
                Some(hello) => {
                    stream_connect.ssl_domain = self.ssl_domain.clone();
                    stream_connect.remote_domain = Some(hello.domain.clone());
                    stream_connect.local_domain = Some(hello.domain.clone());
                    hello
                }
                None => {
                    let domain = protopack::ssl_hello::read_domain(&mut client_buf_reader).await?;
                    let domain = match domain {
                        Some(domain) => domain,
                        None => {
                            if self.ssl_domain.is_none() {
                                return Err(anyhow::anyhow!("err:domain null"));
                            }
                            self.ssl_domain.clone().unwrap()
                        }
                    };
                    stream_connect.ssl_domain = Some(domain.clone());
                    stream_connect.remote_domain = Some(domain.clone());
                    stream_connect.local_domain = Some(domain.clone());

                    protopack::anyproxy::AnyproxyHello {
                        version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                        request_id: format!(
                            "{}{}",
                            stream_connect.remote_addr,
                            Local::now().timestamp()
                        ),
                        client_addr: stream_connect.remote_addr.clone(),
                        domain: domain,
                    }
                }
            }
        };
        stream_connect.protocol_hello = Some(hello);
        StreamStream::work_time(
            "anyproxy::hello",
            self.domain_config_listen.stream.stream_work_times,
            stream_connect,
        );

        let domain_index = self
            .domain_config_listen
            .domain_index
            .index(&stream_connect.protocol_hello.as_ref().unwrap().domain)?;
        let domain_config_context = self
            .domain_config_listen
            .domain_config_context_map
            .get(&domain_index)
            .cloned()
            .ok_or(anyhow::anyhow!(
                "err:domain_index nil => domain_index:{}",
                domain_index
            ))?;

        self.domain_config_context = Some(domain_config_context);
        let domain_config_context = self.domain_config_context.as_ref().unwrap();

        let (
            upstream_protocol_name,
            mut upstream_stream,
            upstream_addr,
            upstream_elapsed,
            _local_addr,
            _remote_addr,
        ) = {
            stream_connect.err_status = stream_connect::ErrStatus::ServiceUnavailable;
            domain_config_context
                .connect
                .connect(&mut Some(
                    &mut stream_connect.upstream_connect_info.borrow_mut(),
                ))
                .await?
        };
        let read_timeout =
            tokio::time::Duration::from_secs(domain_config_context.stream.stream_recv_timeout);
        let write_timeout =
            tokio::time::Duration::from_secs(domain_config_context.stream.stream_send_timeout);
        upstream_stream.set_config(
            read_timeout,
            write_timeout,
            true,
            Some(stream_connect.upstream_stream_info.clone()),
        );
        stream_connect.upstream_protocol_name = Some(upstream_protocol_name.to_string());
        stream_connect.upstream_addr = Some(upstream_addr);
        stream_connect.upstream_connect_time = Some(upstream_elapsed);
        StreamStream::work_time(
            "client::connect",
            domain_config_context.stream.stream_work_times,
            stream_connect,
        );

        stream_connect.err_status = stream_connect::ErrStatus::Ok;
        if domain_config_context.proxy_protocol {
            let mut upstream_buf_writer = tokio::io::BufWriter::new(&upstream_stream);
            protopack::anyproxy::write_pack(
                &mut upstream_buf_writer,
                protopack::anyproxy::AnyproxyHeaderType::Hello,
                stream_connect.protocol_hello.as_ref().unwrap(),
            )
            .await?;
        }
        if client_buf_reader.buffer().len() > 0 {
            upstream_stream.write(client_buf_reader.buffer()).await?;
        }

        client_stream.set_config(
            read_timeout,
            write_timeout,
            true,
            Some(stream_connect.client_stream_info.clone()),
        );

        let ret = StreamStream::stream_to_stream(
            domain_config_context.stream.stream_cache_size,
            domain_config_context.stream.stream_work_times,
            stream_connect,
            &mut client_stream,
            &mut upstream_stream,
        )
        .await;

        StreamStream::work_time(
            "stream_to_stream",
            domain_config_context.stream.stream_work_times,
            stream_connect,
        );
        ret?;
        Ok(())
    }
}
