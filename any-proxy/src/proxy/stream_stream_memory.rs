use super::stream_info::StreamInfo;
use super::StreamCacheBuffer;
use super::StreamLimit;
use super::StreamStreamContext;
use crate::proxy::stream_stream::{StreamStatus, StreamStream};
use crate::util::default_config;
use any_base::io::async_read_msg::AsyncReadMsg;
use any_base::io::async_read_msg::AsyncReadMsgExt;
use any_base::io::async_write_msg::AsyncWriteMsg;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

const LIMIT_SLEEP_TIME_MILLIS: u64 = 100;

pub struct StreamStreamMemory {}

impl StreamStreamMemory {
    pub fn new() -> StreamStreamMemory {
        StreamStreamMemory {}
    }
    pub async fn start<
        R: AsyncRead + AsyncReadMsg + Unpin,
        W: AsyncWrite + AsyncWriteMsg + Unpin,
    >(
        &mut self,
        limit: StreamLimit,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: R,
        mut w: W,
        is_client: bool,
    ) -> Result<()> {
        stream_info.borrow_mut().buffer_cache = Some("memory".to_string());
        stream_info
            .borrow_mut()
            .add_work_time(&format!("{} cache", StreamStream::get_flag(is_client)));

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = StreamStreamContext {
            stream_cache_size: 0,
            tmp_file_size: 0,
            limit_rate_after: limit.limit_rate_after as i64,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: 0,
            max_tmp_file_size: 0,
            is_tmp_file_io_page: true,
            page_size,
            min_merge_cache_buffer_size: (page_size / 2) as u64,
            min_cache_buffer_size: page_size * *default_config::MIN_CACHE_BUFFER_NUM,
            min_read_buffer_size: page_size,
            min_cache_file_size: page_size * 256,
            total_read_size: 0,
            total_write_size: 0,
        };

        let mut is_first = true;
        let mut buffer = StreamCacheBuffer::new();
        buffer.is_cache = true;
        loop {
            buffer.reset();
            buffer.resize(None);
            buffer.is_cache = true;

            if r.is_read_msg().await {
                let msg = r.read_msg().await?;
                buffer.size = msg.len() as u64;
                buffer.msg = Some(msg);
            } else {
                buffer.size =
                    r.read(&mut buffer.data)
                        .await
                        .map_err(|e| anyhow!("err:r.read => e:{}", e))? as u64;
            }
            ssc.total_read_size += buffer.size;
            stream_info.borrow_mut().total_read_size += buffer.size;

            if stream_info.borrow().debug_is_open_print {
                let stream_info = stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:read buffer.size:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(is_client),
                    buffer.size,
                );
            }

            if is_first {
                is_first = false;
                stream_info
                    .borrow_mut()
                    .add_work_time(StreamStream::get_flag(is_client));
            }

            loop {
                if buffer.start >= buffer.size {
                    break;
                }

                if stream_info.borrow().debug_is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{:?}---{}:write start:{}, size:{}",
                        stream_info.request_id,
                        stream_info.server_stream_info.local_addr,
                        StreamStream::get_flag(is_client),
                        buffer.start,
                        buffer.size
                    );
                }

                let stream_status = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut ssc,
                    &mut w,
                    is_client,
                    #[cfg(unix)]
                    &None,
                    stream_info.clone(),
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;
                if stream_info.borrow().debug_is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{:?}---{}: stream_status:{:?}",
                        stream_info.request_id,
                        stream_info.server_stream_info.local_addr,
                        StreamStream::get_flag(is_client),
                        stream_status
                    );
                }

                if let StreamStatus::Limit = stream_status {
                    tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS))
                        .await;
                }
            }
        }
    }
}
