use super::stream_info::StreamInfo;
use crate::protopack;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::typ::Share;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::sync::Arc;

pub struct TunnelStream {}

impl TunnelStream {
    pub async fn tunnel_stream(
        tunnel_publish: Option<tunnel_server::Publish>,
        tunnel2_publish: Option<tunnel2_server::Publish>,
        server_stream_info: Arc<ServerStreamInfo>,
        mut client_buf_reader: any_base::io_rb::buf_reader::BufReader<stream_flow::StreamFlow>,
        stream_info: Share<StreamInfo>,
        executors: ExecutorsLocal,
    ) -> Result<Option<any_base::io_rb::buf_reader::BufReader<stream_flow::StreamFlow>>> {
        log::debug!(
            "server protocol7:{}",
            server_stream_info.protocol7.to_string()
        );
        if server_stream_info.protocol7.is_tunnel() {
            return Ok(Some(client_buf_reader));
        }

        let tunnel_hello = protopack::anyproxy::read_tunnel_hello(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:read_tunnel_hello => e:{}", e))?;

        if tunnel_hello.is_some() {
            if tunnel_publish.is_none() {
                return Err(anyhow::anyhow!("tunnel_publish nil"));
            }
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.get_mut().is_discard_flow = true;
            stream_info.get_mut().is_discard_timeout = true;
            let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                any_base::io::buf_writer::BufWriter::new(client_buf_reader.to_io_buf_reader()),
            );
            //let (r, w) = tokio::io::split(client_buf_stream);
            // tunnel_publish
            //     .push_peer_stream(
            //         Box::new(r),
            //         Box::new(w),
            //         server_stream_info.local_addr.clone().unwrap(),
            //         server_stream_info.remote_addr,
            //     )
            //     .await
            //     .map_err(|e| anyhow!("err:self.tunnel_publish.push_peer_stream => e:{}", e))?;

            let mut shutdown_thread_rx = executors.context.shutdown_thread_tx.subscribe();
            let tunnel_publish = tunnel_publish.unwrap();
            executors._start_and_free(move |_| async move {
                async {
                    tokio::select! {
                        biased;
                        ret = tunnel_publish.push_peer_stream(
                           client_buf_stream,
                            server_stream_info.local_addr.clone().unwrap(),
                            server_stream_info.remote_addr,
                            server_stream_info.domain.clone(),
                        ) => {
                            ret.map_err(|e| anyhow!("err:self.tunnel_publish.push_peer_stream => e:{}", e))?;
                            return Ok(());
                        }
                        _ = shutdown_thread_rx.recv() => {
                            return Ok(());
                        }
                        else => {
                            return Err(anyhow!("tokio::select"));
                        }
                    }
                }.await
            });

            return Ok(None);
        }

        let tunnel_hello = protopack::anyproxy::read_tunnel2_hello(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:anyproxy::read_tunnel2_hello => e:{}", e))?;

        if tunnel_hello.is_some() {
            if tunnel2_publish.is_none() {
                return Err(anyhow::anyhow!("tunnel2_publish nil"));
            }
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.get_mut().is_discard_flow = true;
            stream_info.get_mut().is_discard_timeout = true;
            let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                any_base::io::buf_writer::BufWriter::new(client_buf_reader.to_io_buf_reader()),
            );
            //let (r, w) = tokio::io::split(client_buf_stream);
            // tunnel2_publish
            //     .push_peer_stream(
            //         Box::new(r),
            //         Box::new(w),
            //         server_stream_info.local_addr.clone().unwrap(),
            //         server_stream_info.remote_addr,
            //     )
            //     .await
            //     .map_err(|e| anyhow!("err:self.tunnel2_publish.push_peer_stream => e:{}", e))?;

            let mut shutdown_thread_rx = executors.context.shutdown_thread_tx.subscribe();
            let tunnel2_publish = tunnel2_publish.unwrap();
            executors._start_and_free(move |_| async move {
                async {
                    tokio::select! {
                        biased;
                        ret = tunnel2_publish.push_peer_stream(
                            client_buf_stream,
                            server_stream_info.local_addr.clone().unwrap(),
                            server_stream_info.remote_addr,
                        ) => {
                            ret.map_err(|e| anyhow!("err:self.tunnel2_publish.push_peer_stream => e:{}", e))?;
                            return Ok(());
                        }
                        _ = shutdown_thread_rx.recv() => {
                            return Ok(());
                        }
                        else => {
                            return Err(anyhow!("tokio::select"));
                        }
                    }
                }.await
            });

            return Ok(None);
        }

        Ok(Some(client_buf_reader))
    }
}
