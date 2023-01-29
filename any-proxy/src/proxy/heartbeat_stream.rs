use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use crate::io::buf_reader::BufReader;
use crate::io::buf_stream::BufStream;
use crate::io::buf_writer::BufWriter;
use crate::protopack;
use crate::stream::stream_flow;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

pub struct HeartbeatStream {}

impl HeartbeatStream {
    pub async fn heartbeat_stream(
        mut client_buf_reader: BufReader<stream_flow::StreamFlow>,
        stream_info: Rc<RefCell<StreamInfo>>,
    ) -> Result<(BufReader<stream_flow::StreamFlow>, Rc<RefCell<StreamInfo>>)> {
        let mut heartbeat = protopack::anyproxy::read_heartbeat_rollback(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:anyproxy::read_heartbeat => e:{}", e))?;
        if heartbeat.is_none() {
            return Ok((client_buf_reader, stream_info));
        }

        stream_info.borrow_mut().err_status = ErrStatus::Ok;
        stream_info.borrow_mut().is_discard_flow = true;
        stream_info.borrow_mut().is_discard_timeout = true;

        let mut client_buf_stream = BufStream::from(BufWriter::new(client_buf_reader));

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
                stream_info.borrow().remote_addr
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
