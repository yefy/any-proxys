use super::StreamCacheBuffer;
use crate::config::http_core_plugin::PluginHandleStream;
#[cfg(unix)]
use crate::proxy::SENDFILE_WRITEABLE_MILLIS;
use crate::proxy::{get_flag, StreamStatus, StreamStatusFull, StreamStreamShare};
#[cfg(unix)]
use any_base::io::async_stream::AsyncStreamExt;
use any_base::io::async_write_msg::AsyncWriteMsgExt;
use any_base::typ::{ArcRwLockTokio, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use dynamic_pool::DynamicPoolItem;
use lazy_static::lazy_static;
use std::sync::atomic::Ordering;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::AsyncWriteExt;

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

pub async fn cache_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss, CACHE_HANDLE_NEXT.clone()).await
}

pub async fn memory_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss, MEMORY_HANDLE_NEXT.clone()).await
}

pub async fn handle_run(
    sss: ShareRw<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<usize> {
    let mut buffer = sss.get_mut().write_buffer.take().unwrap();
    let ret = do_handle_run(sss.clone(), &mut buffer).await;
    if let Err(ref e) = ret {
        log::debug!("{}, write_err = Some(ret)", get_flag(sss.get().is_client));
        if e.kind() != std::io::ErrorKind::ConnectionReset {
            return Err(anyhow!("err:StreamStream.stream_write => e:{}", e))?;
        }
        sss.get_mut().write_err = Some(Err(anyhow!(
            "err:stream_write => flag:{}, e:{}",
            get_flag(sss.get().is_client),
            e
        )));
        sss.get_mut().stream_status = StreamStatus::Full(StreamStatusFull::default());
    } else {
        let status = ret?;
        sss.get_mut().stream_status = status;
    }

    if buffer.start < buffer.size {
        sss.get_mut().write_buffer = Some(buffer);
    }

    if handle.is_some().await {
        let handle_next = &*handle.get().await;
        (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:write_buffer => e:{}", e))
    } else {
        Ok(0)
    }
}

pub async fn do_handle_run(
    sss: ShareRw<StreamStreamShare>,
    buffer: &mut DynamicPoolItem<StreamCacheBuffer>,
) -> std::io::Result<StreamStatus> {
    let cs = sss.get().ssc.cs.clone();
    let ssd = sss.get().ssc.ssd.clone();
    let stream_info = sss.get().stream_info.clone();
    let curr_limit_rate = sss.get().ssc.curr_limit_rate.clone();

    let mut _is_sendfile = false;
    let mut n = buffer.size - buffer.start;
    let curr_limit_rate_num = curr_limit_rate.load(Ordering::Relaxed);
    let limit_rate_size = if cs.max_limit_rate <= 0 {
        n
    } else if ssd.get().limit_rate_after > 0 {
        ssd.get().limit_rate_after as u64
    } else if curr_limit_rate_num > 0 {
        curr_limit_rate_num as u64
    } else {
        return Ok(StreamStatus::Limit);
    };

    if limit_rate_size < n {
        n = limit_rate_size;
    }
    log::trace!("{}, n:{}", get_flag(sss.get().is_client), n);

    let w = sss.get().w.clone();
    let mut w = w.get_mut().await;
    let wn = loop {
        #[cfg(unix)]
        let sendfile = sss.get().sendfile.clone();
        #[cfg(unix)]
        if sendfile.get().await.is_some() && buffer.file_fd > 0 {
            _is_sendfile = true;
            let timeout = tokio::time::Duration::from_millis(SENDFILE_WRITEABLE_MILLIS);
            match tokio::time::timeout(timeout, w.writable()).await {
                Ok(ret) => {
                    ret?;
                }
                Err(_) => {
                    break 0;
                }
            }
            let wn = sendfile
                .get()
                .await
                .write(buffer.file_fd, buffer.seek, n)
                .await;
            if let Err(e) = wn {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(StreamStatus::Full(StreamStatusFull::new(_is_sendfile, 0)));
                }
                return Err(e);
            }
            break wn.unwrap();
        }

        _is_sendfile = false;
        let start = buffer.start as usize;
        let end = (buffer.start + n) as usize;

        log::trace!("write:{:?}", buffer.data(start, end));
        #[cfg(feature = "anyproxy-write-block-time-ms")]
        let write_start_time = Instant::now();

        let write_size = if w.is_write_msg().await {
            let msg = buffer.msg(start, end);
            w.write_msg(msg).await? as u64
        } else {
            w.write(buffer.data(start, end)).await? as u64
        };

        #[cfg(feature = "anyproxy-write-block-time-ms")]
        {
            let write_max_block_time_ms = write_start_time.elapsed().as_millis();
            if write_max_block_time_ms > stream_info.get().write_max_block_time_ms {
                stream_info.get_mut().write_max_block_time_ms = write_max_block_time_ms;
            }
        }
        break write_size;
    };

    if sss.get().is_first_write {
        sss.get_mut().is_first_write = false;
        stream_info
            .get_mut()
            .add_work_time(&format!("first write {}", get_flag(sss.get().is_client)));
    }

    ssd.get_mut().total_write_size += wn;
    stream_info.get_mut().total_write_size += wn;
    log::trace!("{}, wn:{}", get_flag(sss.get().is_client), wn);
    if cs.max_limit_rate <= 0 {
        //
    } else if ssd.get().limit_rate_after > 0 {
        ssd.get_mut().limit_rate_after -= wn as i64;
    } else {
        curr_limit_rate.fetch_sub(wn as i64, Ordering::Relaxed);
    }
    buffer.start += wn;
    buffer.seek += wn;
    if buffer.is_cache {
        if cs.max_stream_cache_size > 0 {
            ssd.get_mut().stream_cache_size += wn as i64;
        }
    } else {
        if cs.max_tmp_file_size > 0 {
            ssd.get_mut().tmp_file_size += wn as i64;
        }
    }

    if wn != n {
        return Ok(StreamStatus::Full(StreamStatusFull::new(_is_sendfile, wn)));
    }
    return Ok(StreamStatus::Ok(wn));
}
