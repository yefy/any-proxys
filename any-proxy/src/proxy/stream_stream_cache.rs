use super::StreamCacheBuffer;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::{get_flag, StreamStatus};
use crate::proxy::{
    StreamStreamShare, LIMIT_SLEEP_TIME_MILLIS, MIN_SLEEP_TIME_MILLIS, NORMAL_SLEEP_TIME_MILLIS,
    NOT_SLEEP_TIME_MILLIS, SENDFILE_FULL_SLEEP_TIME_MILLIS,
};
use any_base::io::async_read_msg::AsyncReadMsgExt;
use any_base::io::async_stream::AsyncStreamExt;
use any_base::typ::{ArcRwLockTokio, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use dynamic_pool::{DynamicPool, DynamicPoolItem};
use lazy_static::lazy_static;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::AsyncReadExt;

lazy_static! {
    pub static ref CACHE_HANDLE_NEXT: ArcRwLockTokio<PluginHandleStream> =
        ArcRwLockTokio::default();
}

pub async fn set_cache_handle(plugin: PluginHandleStream) -> Result<()> {
    if CACHE_HANDLE_NEXT.is_none().await {
        CACHE_HANDLE_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn cache_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss).await
}

pub async fn handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    let cs = sss.get().ssc.cs.clone();
    let ssd = sss.get().ssc.ssd.clone();
    let stream_info = sss.get().stream_info.clone();

    stream_info.get_mut().buffer_cache = Some(cs.buffer_cache.clone());
    stream_info
        .get_mut()
        .add_work_time(&format!("{} cache", get_flag(sss.get().is_client)));

    {
        let buffer_size = cs.min_cache_buffer_size as i64;
        let maximum_capacity = std::cmp::max(cs.stream_cache_size, buffer_size * 4);

        let initial_capacity = 1;
        let maximum_capacity = (maximum_capacity / buffer_size) as usize + initial_capacity;
        let buffer_pool = DynamicPool::new(
            initial_capacity,
            maximum_capacity,
            StreamCacheBuffer::default,
        );
        sss.get_mut().buffer_pool.set(buffer_pool);
    }

    let read_buffer_size = sss.get().ssc.cs.read_buffer_size;
    let mut is_first = true;
    loop {
        if (ssd.get().stream_cache_size > 0 || ssd.get().tmp_file_size > 0)
            && sss.get().read_err.is_none()
        {
            let (ret, buffer) = read(sss.clone(), read_buffer_size)
                .await
                .map_err(|e| anyhow!("err:read => e:{}", e))?;
            if is_first {
                is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time(&format!("first read {}", get_flag(sss.get().is_client)));
            }
            sss.get_mut().read_buffer = Some(buffer);
            sss.get_mut().read_buffer_ret = Some(ret);
        } else {
            let stream_status = sss.get().stream_status.clone();
            StreamStreamShare::stream_status_sleep(stream_status).await;
            if sss.get().read_err.is_some() && sss.get().read_buffer.is_some() {
                if sss.get().read_buffer.as_ref().unwrap().read_size <= 0 {
                    log::error!("err:buffer.read_size <= 0");
                } else {
                    sss.get_mut().read_buffer_ret = Some(Ok(0));
                }
            }
        }
        let handle_next = &*CACHE_HANDLE_NEXT.get().await;
        (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;

        check_stream_close(sss.clone()).await?;
    }
}

pub async fn check_stream_close(sss: ShareRw<StreamStreamShare>) -> Result<()> {
    let cs = sss.get().ssc.cs.clone();
    let stream_info = sss.get().stream_info.clone();
    stream_info.get_mut().buffer_cache = Some(cs.buffer_cache.clone());

    let r = sss.get_mut().r.clone();
    let is_single = r.get_mut().await.is_single().await;
    let is_sendfile_close = StreamStreamShare::is_sendfile_close(sss.clone()).await;
    if sss.get().is_fast_close {
        let mut sss = sss.get_mut();
        if sss.write_err.is_some() {
            sss.write_err.take().unwrap()?;
        }

        if sss.read_err.is_some() && sss.is_read_empty() && sss.is_write_empty() {
            sss.read_err.take().unwrap()?;
        }
    }

    let is_close = {
        let sss = sss.get_mut();
        sss.write_err.is_some()
            || (sss.read_err.is_some() && sss.is_read_empty() && sss.is_write_empty())
            || (!is_single
                && stream_info.get().close_num >= 1
                && sss.is_read_empty()
                && sss.is_write_empty())
            || is_sendfile_close
    };

    if is_close {
        use std::time::Instant;
        let start_time = Instant::now();
        scopeguard::defer! {
            let elapsed = start_time.elapsed().as_millis();
            if elapsed > 500 {
                log::warn!("stream_stream_cache_or_file exit time:{}", elapsed);
            }
        }

        stream_info.get_mut().close_num += 1;
        loop {
            if stream_info.get().close_num >= 2 {
                let mut sss = sss.get_mut();
                if sss.write_err.is_some() {
                    sss.write_err.take().unwrap()?;
                }

                if sss.read_err.is_some() {
                    sss.read_err.take().unwrap()?;
                }
            }

            loop {
                {
                    let other_read = if !sss.get().is_client {
                        stream_info.get().upload_read.clone()
                    } else {
                        stream_info.get().download_read.clone()
                    };
                    other_read.waker();

                    let close_wait = if !sss.get().is_client {
                        stream_info.get().upload_close.clone()
                    } else {
                        stream_info.get().download_close.clone()
                    };
                    close_wait.waker();
                }

                let close_wait = if sss.get().is_client {
                    stream_info.get().upload_close.clone()
                } else {
                    stream_info.get().download_close.clone()
                };

                let ret: std::io::Result<usize> = async {
                    tokio::select! {
                        biased;
                        _= tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                            return Ok(0);
                        },
                        _= close_wait => {
                            return Ok(1);
                        },
                        else => {
                            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                            "err:close_wait select close"
                        )));
                        }
                    }
                }
                .await;

                if ret.unwrap() == 1 {
                    break;
                }
            }
        }
    }
    Ok(())
}

pub async fn read(
    sss: ShareRw<StreamStreamShare>,
    read_buffer_size: usize,
) -> Result<(std::io::Result<usize>, DynamicPoolItem<StreamCacheBuffer>)> {
    let ssd = sss.get().ssc.ssd.clone();
    let stream_info = sss.get().stream_info.clone();

    let mut buffer = if sss.get_mut().read_buffer.is_some() {
        sss.get_mut().read_buffer.take().unwrap()
    } else {
        sss.get_mut().buffer_pool.get().take()
    };

    buffer.resize(None);
    buffer.is_cache = false;

    if buffer.size < buffer.read_size {
        log::error!("err:buffer.size < buffer.read_size");
        return Err(anyhow!("err:buffer.size < buffer.read_size"));
    }

    if ssd.get().stream_cache_size < buffer.read_size as i64
        && ssd.get().tmp_file_size < buffer.read_size as i64
    {
        log::error!("err:buffer.read_size");
        return Err(anyhow!("err:buffer.read_size"));
    }

    if ssd.get().stream_cache_size > 0 && ssd.get().stream_cache_size >= buffer.read_size as i64 {
        buffer.is_cache = true;
        if ssd.get().stream_cache_size < buffer.size as i64 {
            buffer.size = ssd.get().stream_cache_size as u64;
        }
    }

    if !buffer.is_cache {
        if ssd.get().tmp_file_size < buffer.size as i64 {
            buffer.size = ssd.get().tmp_file_size as u64;
        }
    }

    let ret: std::io::Result<usize> = async {
        if buffer.size == buffer.read_size {
            //___test___
            log::warn!(
                "buffer.size:{} == buffer.read_size:{}",
                buffer.size,
                buffer.read_size
            );
            if sss.get().is_write_empty() {
                return Ok(0);
            }
            let stream_status = sss.get().stream_status.clone();
            StreamStreamShare::stream_status_sleep(stream_status).await;
            return Ok(0);
        }

        let sleep_read_time_millis = match &sss.get().stream_status {
            StreamStatus::Limit => LIMIT_SLEEP_TIME_MILLIS,
            StreamStatus::Full(info) => {
                if info.is_sendfile {
                    SENDFILE_FULL_SLEEP_TIME_MILLIS
                } else {
                    NOT_SLEEP_TIME_MILLIS
                }
            }
            StreamStatus::Ok(_) => NOT_SLEEP_TIME_MILLIS,
            StreamStatus::DataEmpty => {
                if buffer.read_size > 0 {
                    MIN_SLEEP_TIME_MILLIS
                } else {
                    NORMAL_SLEEP_TIME_MILLIS
                }
            }
        };

        let read_size = buffer.read_size as usize;
        let end_size = buffer.size as usize;

        let read_wait = if sss.get().is_client {
            stream_info.get().upload_read.clone()
        } else {
            stream_info.get().download_read.clone()
        };
        let r = sss.get().r.clone();
        let mut r = r.get_mut().await;
        if r.is_read_msg().await {
            tokio::select! {
                biased;
                ret = r.read_msg(read_buffer_size) => {
                    let msg = ret?;
                    let n = msg.len();
                    buffer.msg = Some(msg);
                    return Ok(n);
                },
                _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                    return Ok(0);
                },
                 _= read_wait => {
                    return Ok(0);
                },
                else => {
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                    "err:do_stream_to_stream_or_file select close"
                )));
                }
            }
        } else {
            tokio::select! {
                biased;
                ret = r.read(buffer.data(read_size, end_size)) => {
                    let n = ret?;
                    return Ok(n);
                },
                _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                    return Ok(0);
                },
                 _= read_wait => {
                    return Ok(0);
                },
                else => {
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                    "err:do_stream_to_stream_or_file select close"
                )));
                }
            }
        }
    }
    .await;

    Ok((ret, buffer))
}
