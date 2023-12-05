use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use crate::protopack;
use crate::protopack::anyproxy::AnyproxyHeartbeat;
use crate::stream::stream_flow;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::typ::Share;
use anyhow::anyhow;
use anyhow::Result;

pub struct HeartbeatStream {}

impl HeartbeatStream {
    pub async fn heartbeat_stream(
        mut client_buf_reader: any_base::io::buf_reader::BufReader<stream_flow::StreamFlow>,
        stream_info: Share<StreamInfo>,
        executors: ExecutorsLocal,
    ) -> Result<
        Option<(
            any_base::io::buf_reader::BufReader<stream_flow::StreamFlow>,
            Share<StreamInfo>,
        )>,
    > {
        let heartbeat = protopack::anyproxy::read_heartbeat_rollback(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:anyproxy::read_heartbeat => e:{}", e))?;
        if heartbeat.is_none() {
            return Ok(Some((client_buf_reader, stream_info)));
        }

        let mut shutdown_thread_rx = executors.shutdown_thread_tx.subscribe();
        executors._start_and_free(move |_| async move {
            async {
                tokio::select! {
                    biased;
                    ret = HeartbeatStream::do_heartbeat_stream(client_buf_reader, stream_info, heartbeat) => {
                        ret.map_err(|e| anyhow!("err:do_heartbeat_stream => e:{}", e))?;
                        return Ok(());
                    }
                    _ = shutdown_thread_rx.recv() => {
                        return Ok(());
                    }
                    else => {
                        return Err(anyhow!("tokio::select"));
                    }
                }
            }
            .await
        });

        Ok(None)
    }

    pub async fn do_heartbeat_stream(
        client_buf_reader: any_base::io::buf_reader::BufReader<stream_flow::StreamFlow>,
        stream_info: Share<StreamInfo>,
        mut heartbeat: Option<AnyproxyHeartbeat>,
    ) -> Result<()> {
        stream_info.get_mut().err_status = ErrStatus::Ok;
        stream_info.get_mut().is_discard_flow = true;
        stream_info.get_mut().is_discard_timeout = true;

        let mut client_buf_stream = any_base::io::buf_stream::BufStream::from(
            any_base::io::buf_writer::BufWriter::new(client_buf_reader),
        );

        loop {
            let heartbeat = if heartbeat.is_none() {
                protopack::anyproxy::read_heartbeat(&mut client_buf_stream)
                    .await
                    .map_err(|e| anyhow!("err:anyproxy::read_heartbeat => e:{}", e))?
            } else {
                heartbeat.take()
            };

            if heartbeat.is_none() {
                Err(anyhow!("read_heartbeat none"))?
            }

            let heartbeat = heartbeat.unwrap();
            log::debug!(
                "heartbeat:{:?}, remote_addr:{}",
                heartbeat,
                stream_info.get().server_stream_info.remote_addr
            );

            protopack::anyproxy::write_pack(
                &mut client_buf_stream,
                protopack::anyproxy::AnyproxyHeaderType::HeartbeatR,
                &protopack::anyproxy::AnyproxyHeartbeatR {
                    version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                },
            )
            .await
            .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
        }
    }
}
