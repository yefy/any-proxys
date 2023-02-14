use super::stream_info::StreamInfo;
use crate::protopack;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct TunnelStream {}

impl TunnelStream {
    pub async fn tunnel_stream(
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        server_stream_info: Arc<ServerStreamInfo>,
        mut client_buf_reader: any_base::io::buf_reader::BufReader<stream_flow::StreamFlow>,
        stream_info: Rc<RefCell<StreamInfo>>,
        executors: ExecutorsLocal,
    ) -> Result<Option<any_base::io::buf_reader::BufReader<stream_flow::StreamFlow>>> {
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
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.borrow_mut().is_discard_flow = true;
            stream_info.borrow_mut().is_discard_timeout = true;
            let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                any_base::io::buf_writer::BufWriter::new(client_buf_reader),
            );
            let (r, w) = tokio::io::split(client_buf_stream);
            // tunnel_publish
            //     .push_peer_stream(
            //         Box::new(r),
            //         Box::new(w),
            //         server_stream_info.local_addr.clone().unwrap(),
            //         server_stream_info.remote_addr,
            //     )
            //     .await
            //     .map_err(|e| anyhow!("err:self.tunnel_publish.push_peer_stream => e:{}", e))?;

            let mut shutdown_thread_rx = executors.shutdown_thread_tx.subscribe();
            executors._start_and_free(move |_| async move {
                async {
                    tokio::select! {
                        biased;
                        ret = tunnel_publish.push_peer_stream(
                            Box::new(r),
                            Box::new(w),
                            server_stream_info.local_addr.clone().unwrap(),
                            server_stream_info.remote_addr,
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
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.borrow_mut().is_discard_flow = true;
            stream_info.borrow_mut().is_discard_timeout = true;
            let client_buf_stream = any_base::io::buf_stream::BufStream::from(
                any_base::io::buf_writer::BufWriter::new(client_buf_reader),
            );
            let (r, w) = tokio::io::split(client_buf_stream);
            // tunnel2_publish
            //     .push_peer_stream(
            //         Box::new(r),
            //         Box::new(w),
            //         server_stream_info.local_addr.clone().unwrap(),
            //         server_stream_info.remote_addr,
            //     )
            //     .await
            //     .map_err(|e| anyhow!("err:self.tunnel2_publish.push_peer_stream => e:{}", e))?;

            let mut shutdown_thread_rx = executors.shutdown_thread_tx.subscribe();
            executors._start_and_free(move |_| async move {
                async {
                    tokio::select! {
                        biased;
                        ret = tunnel2_publish.push_peer_stream(
                            Box::new(r),
                            Box::new(w),
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
