use crate::config::net_core_plugin::PluginHandleStream;
use crate::proxy::stream_stream_buf::StreamStreamCacheWriteDeque;
use crate::proxy::{StreamStatus, StreamStreamShare};
use any_base::io::async_write_msg::{AsyncWriteMsg, AsyncWriteMsgExt, MsgWriteBuf};
use any_base::typ::ArcRwLockTokio;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::io::IoSlice;
use std::sync::atomic::Ordering;
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::AsyncWriteExt;

const MAX_WRITEV_BUFS: usize = 64;

lazy_static! {
    pub static ref CACHE_HANDLE_NEXT: ArcRwLockTokio<PluginHandleStream> =
        ArcRwLockTokio::default();
    pub static ref MEMORY_HANDLE_NEXT: ArcRwLockTokio<PluginHandleStream> =
        ArcRwLockTokio::default();
}

pub async fn set_cache_handle(plugin: PluginHandleStream) -> Result<()> {
    if CACHE_HANDLE_NEXT.is_none().await {
        CACHE_HANDLE_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn set_memory_handle(plugin: PluginHandleStream) -> Result<()> {
    if MEMORY_HANDLE_NEXT.is_none().await {
        MEMORY_HANDLE_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn cache_handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss, CACHE_HANDLE_NEXT.clone()).await
}

pub async fn memory_handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss, MEMORY_HANDLE_NEXT.clone()).await
}

pub async fn handle_run(
    sss: Arc<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();

    let mut write_buffer_deque = {
        let (buffer, is_deque_none) = {
            let mut sss_ctx = sss.sss_ctx.get_mut();
            (
                sss_ctx.write_buffer.take(),
                sss_ctx.write_buffer_deque.as_ref().unwrap().is_none(),
            )
        };
        if buffer.is_none() && is_deque_none {
            let mut w = sss.w.get_mut().await;
            w.flush().await?;
            return Ok(StreamStatus::WriteEmpty);
        }
        let mut sss_ctx = sss.sss_ctx.get_mut();

        let write_size = if sss.is_write_msg {
            ssc.cs.read_buffer_size
        } else {
            let write_buffer_deque = sss_ctx.write_buffer_deque.as_ref().unwrap();
            let is_file = write_buffer_deque.is_file();
            if is_file {
                usize::max_value()
            } else {
                if sss.is_write_vectored {
                    ssc.cs.write_buffer_size
                } else {
                    ssc.cs.write_buffer_size
                }
            }
        };

        let mut write_buffer_deque = sss_ctx.write_buffer_deque.take().unwrap();
        if buffer.is_some() {
            let buffer = buffer.unwrap();

            #[cfg(unix)]
            use any_base::io::async_write_msg::BufFile;
            #[cfg(unix)]
            if buffer.is_file() && sss.ssc.cs.stream_nopush {
                if !sss_ctx.is_set_tcp_nopush {
                    sss_ctx.is_set_tcp_nopush = true;
                    use any_base::util::set_tcp_nopush_;
                    if sss._w_raw_fd > 0 {
                        set_tcp_nopush_(sss._w_raw_fd, true);
                    }
                }
            }
            let buffer = write_buffer_deque.push_back(write_size, buffer);
            if buffer.is_some() {
                sss_ctx.write_buffer = buffer;
            } else {
                if !write_buffer_deque.is_full() && write_buffer_deque.remaining_() < write_size {
                    sss_ctx.write_buffer_deque = Some(write_buffer_deque);
                    return Ok(StreamStatus::Ok);
                }
            }
        }
        write_buffer_deque
    };

    let total_write_size = ssc.ssd.get().total_write_size as i64;
    let status = loop {
        if write_buffer_deque.is_none() {
            break StreamStatus::Ok;
        }

        let ret = do_handle_run(&sss, &mut write_buffer_deque).await;
        let status = if let Err(ref e) = ret {
            let mut sss_ctx = sss.sss_ctx.get_mut();
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                sss_ctx.write_buffer_deque = Some(write_buffer_deque);
                return Err(anyhow!("err:do_handle_run => e:{}", e))?;
            }
            sss_ctx.write_err = Some(Err(anyhow!("err:stream_write => e:{}", e)));
            StreamStatus::Full(false)
        } else {
            ret.unwrap()
        };

        match &status {
            &StreamStatus::Ok => {
                //
            }
            _ => break status,
        }
    };

    sss.sss_ctx.get_mut().write_buffer_deque = Some(write_buffer_deque);

    loop {
        let mut w = sss.w.get_mut().await;
        if w.write_cache_size() > 0 {
            w.flush().await?;
            break;
        }

        if total_write_size < ssc.cs.stream_nodelay_size {
            w.flush().await?;
            break;
        }
        break;
    }

    if handle.is_some().await {
        let handle_next = &*handle.get().await;
        let _ = (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
    }
    Ok(status)
}

//limit ok full
pub async fn do_handle_run(
    sss: &StreamStreamShare,
    write_buffer_deque: &mut StreamStreamCacheWriteDeque,
) -> std::io::Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let ssd = ssc.ssd.clone();
    let stream_info = sss.stream_info.clone();

    let mut remaining = write_buffer_deque.remaining_();
    let curr_limit_rate_num = ssc.curr_limit_rate.load(Ordering::Relaxed);
    let mut limit_rate_size = {
        let ssd = ssd.get();
        if ssc.cs.max_limit_rate <= 0 {
            remaining as i64
        } else if ssd.limit_rate_after > 0 {
            ssd.limit_rate_after
        } else if curr_limit_rate_num > 0 {
            curr_limit_rate_num
        } else {
            return Ok(StreamStatus::Limit);
        }
    };

    if limit_rate_size < 0 {
        limit_rate_size = 0;
    }
    let limit_rate_size = limit_rate_size as usize;

    if limit_rate_size < remaining {
        remaining = limit_rate_size;
    }

    if remaining <= 0 {
        return Ok(StreamStatus::Limit);
    }

    let remaining = write_buffer_deque.remaining_();

    let mut w = sss.w.get_mut().await;

    #[cfg(feature = "anyproxy-write-block-time-ms")]
    let write_start_time = Instant::now();

    let mut is_sendfile = false;
    let wn: std::io::Result<usize> = async {
        if sss.is_write_msg {
            let mut msg_write_buf = write_buffer_deque.msg_write_buf();
            let rx = if let MsgWriteBuf::File(file) = &mut msg_write_buf {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                file.notify_tx.as_mut().unwrap().push(tx);
                Some(rx)
            } else {
                None
            };
            let n = w.write_msg(msg_write_buf).await?;
            if rx.is_some() {
                log::debug!("write wait file start:{}", sss._session_id);
                let _ = rx.unwrap().recv().await;
                log::debug!("write wait file end:{}", sss._session_id);
            }
            return Ok(n);
        }
        if write_buffer_deque.is_file() {
            if w.write_cache_size() > 0 {
                w.flush().await?;
            }
            is_sendfile = true;
            let file_data = write_buffer_deque.file().unwrap();
            return w
                .sendfile(
                    file_data.file_ext.fix.file_fd,
                    file_data.seek,
                    file_data.size,
                )
                .await;
        }
        if sss.is_write_vectored {
            let mut iovs = [IoSlice::new(&[]); MAX_WRITEV_BUFS];
            let len = write_buffer_deque.chunks_vectored(&mut iovs);
            return w.write_vectored(&iovs[..len]).await;
        }
        w.write(write_buffer_deque.chunk_all()).await
    }
    .await;
    let wn = wn?;

    let ret = write_buffer_deque.advance_(wn);

    #[cfg(feature = "anyproxy-write-block-time-ms")]
    {
        let mut stream_info = stream_info.get_mut();
        let write_max_block_time_ms = write_start_time.elapsed().as_millis();
        if write_max_block_time_ms > stream_info.write_max_block_time_ms {
            stream_info.write_max_block_time_ms = write_max_block_time_ms;
        }
    }

    let mut stream_info = stream_info.get_mut();
    let mut ssd = ssd.get_mut();
    if sss.sss_ctx.get().is_first_write {
        sss.sss_ctx.get_mut().is_first_write = false;
        stream_info.add_work_time2(sss.ssc.is_client, "first write");
    }
    if ssc.cs.max_limit_rate <= 0 {
        //
    } else if ssd.limit_rate_after > 0 {
        ssd.limit_rate_after -= wn as i64;
    } else {
        ssc.curr_limit_rate.fetch_sub(wn as i64, Ordering::Relaxed);
    }

    ssd.total_write_size += wn as u64;
    for (is_cache, write_size) in ret {
        if is_cache {
            ssd.stream_cache_size += write_size as i64;
        } else {
            ssd.tmp_file_size += write_size as i64;
        }
    }

    if wn < remaining {
        return Ok(StreamStatus::Full(is_sendfile));
    }
    return Ok(StreamStatus::Ok);
}
