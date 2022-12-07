use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use crate::io::buf_reader::BufReader;
use crate::io::buf_writer::BufWriter;
use crate::protopack;
use crate::stream::stream_flow;
use anyhow::anyhow;
use anyhow::Result;

pub struct HeartbeatStream {}

impl HeartbeatStream {
    pub async fn heartbeat_stream(
        client_stream: &stream_flow::StreamFlow,
        client_buf_reader: &mut BufReader<&stream_flow::StreamFlow>,
        stream_info: &mut StreamInfo,
    ) -> Result<()> {
        let mut is_first = true;
        let mut buf_writer = BufWriter::new(client_stream);
        loop {
            let heartbeat = protopack::anyproxy::read_heartbeat(client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_heartbeat => e:{}", e))?;
            if heartbeat.is_none() {
                if is_first {
                    return Ok(());
                } else {
                    Err(anyhow!("read_heartbeat none"))?
                }
            }
            if is_first {
                stream_info.err_status = ErrStatus::Ok;
                stream_info.is_discard_flow = true;
                is_first = false;
            }
            let heartbeat = heartbeat.unwrap();
            log::debug!(
                "heartbeat:{:?}, remote_addr:{}",
                heartbeat,
                stream_info.remote_addr
            );

            protopack::anyproxy::write_pack(
                &mut buf_writer,
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
