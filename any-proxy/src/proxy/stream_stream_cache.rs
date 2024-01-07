use super::StreamCacheBuffer;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::{get_flag, StreamCloseType, StreamStatus};
use crate::proxy::{StreamStreamShare, LIMIT_SLEEP_TIME_MILLIS, NORMAL_SLEEP_TIME_MILLIS};
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
use tokio::io::AsyncWriteExt;

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

pub async fn cache_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss).await
}

pub async fn handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    let (ssd, stream_info, is_client, read_buffer_size) = {
        let mut sss = sss.get_mut();
        let cs = sss.ssc.cs.clone();
        let stream_info = sss.stream_info.clone();
        let mut stream_info = stream_info.get_mut();

        stream_info.buffer_cache = Some(cs.buffer_cache.clone());
        stream_info.add_work_time(&format!("{} cache", get_flag(sss.ssc.is_client)));

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
            sss.buffer_pool.set(buffer_pool);
        }
        sss.is_stream_cache = true;
        (
            sss.ssc.ssd.clone(),
            sss.stream_info.clone(),
            sss.ssc.is_client,
            cs.read_buffer_size,
        )
    };
    let mut is_first = true;

    let mut stream_status = StreamStatus::DataEmpty;
    loop {
        let is_read = {
            let ssd = ssd.get();
            let sss = sss.get();
            (ssd.stream_cache_size > 0 || ssd.tmp_file_size > 0) && sss.read_err.is_none()
        };
        if is_read {
            let buffer = read(sss.clone(), read_buffer_size, &stream_status)
                .await
                .map_err(|e| anyhow!("err:read => e:{}", e))?;
            let mut sss = sss.get_mut();
            if is_first {
                is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time(&format!("first read {}", get_flag(is_client)));
            }
            sss.read_buffer = Some(buffer);
        } else {
            StreamStreamShare::stream_status_sleep(&stream_status, is_client).await;
        }
        let handle_next = &*CACHE_HANDLE_NEXT.get().await;
        stream_status = (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:CACHE_HANDLE_NEXT => e:{}", e))?;

        check_stream_close(sss.clone(), &stream_status).await?;
    }
}

pub async fn check_stream_close(
    sss: ShareRw<StreamStreamShare>,
    stream_status: &StreamStatus,
) -> Result<()> {
    let (ssc, cs, stream_info, r) = {
        let sss = sss.get();
        (
            sss.ssc.clone(),
            sss.ssc.cs.clone(),
            sss.stream_info.clone(),
            sss.r.clone(),
        )
    };
    stream_info.get_mut().buffer_cache = Some(cs.buffer_cache.clone());

    let is_single = r.get_mut().await.is_single().await;
    let is_sendfile_close = StreamStreamShare::is_sendfile_close(sss.clone(), stream_status).await;

    let ret: Result<()> = async {
        let mut sss = sss.get_mut();
        if let &StreamCloseType::WaitEmpty = &sss.ssc.close_type {
            Ok(())
        } else {
            if sss.write_err.is_some() {
                sss.write_err.take().unwrap()?;
            }

            if sss.read_err.is_some() && sss.is_read_empty() && sss.is_write_empty() {
                sss.read_err.take().unwrap()?;
            }
            Ok(())
        }
    }
    .await;
    if ret.is_err() {
        let (w, ssc) = {
            let sss = sss.get();
            (sss.w.clone(), sss.ssc.clone())
        };
        let mut w = w.get_mut().await;
        let _ = w.flush().await;
        if let &StreamCloseType::Shutdown = &ssc.close_type {
            let _ = w.shutdown().await;
            ssc.worker_inner.done();
            let _ = ssc.wait_group.wait().await;
        }
        ret?;
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
        // use std::time::Instant;
        // let start_time = Instant::now();
        // scopeguard::defer! {
        //     let elapsed = start_time.elapsed().as_millis();
        //     if elapsed > 500 {
        //         log::warn!("stream_stream_cache_or_file exit time:{}", elapsed);
        //     }
        // }
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
                ssc.other_read_wait.clone().waker();
                ssc.other_close_wait.clone().waker();
                tokio::select! {
                    biased;
                    _= tokio::time::sleep(std::time::Duration::from_millis(1000)) => {
                        continue;
                    },
                    _= ssc.close_wait.clone() => {
                        break;
                    },
                    else => {
                       break;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn read(
    sss: ShareRw<StreamStreamShare>,
    read_buffer_size: usize,
    stream_status: &StreamStatus,
) -> Result<DynamicPoolItem<StreamCacheBuffer>> {
    let mut buffer = {
        let mut sss = sss.get_mut();
        let ssd = sss.ssc.ssd.clone();
        let ssd = ssd.get();

        let mut buffer = if sss.read_buffer.is_some() {
            sss.read_buffer.take().unwrap()
        } else {
            let mut buffer = sss.buffer_pool.get().take();
            buffer.resize(None);
            buffer
        };

        if buffer.size <= buffer.read_size {
            log::error!("err:buffer.size < buffer.read_size");
            return Err(anyhow!("err:buffer.size < buffer.read_size"));
        }

        if ssd.stream_cache_size <= buffer.read_size as i64
            && ssd.tmp_file_size <= buffer.read_size as i64
        {
            log::error!("err:buffer.read_size");
            return Err(anyhow!("err:buffer.read_size"));
        }
        let mut is_find = false;
        if ssd.stream_cache_size > 0 && ssd.stream_cache_size > buffer.read_size as i64 {
            if buffer.read_size == 0 || (buffer.read_size > 0 && buffer.is_cache) {
                is_find = true;
                buffer.is_cache = true;
                if ssd.stream_cache_size < buffer.size as i64 {
                    buffer.size = ssd.stream_cache_size as u64;
                }
            }
        }

        if !is_find {
            if ssd.tmp_file_size > 0 && ssd.tmp_file_size > buffer.read_size as i64 {
                if buffer.read_size == 0 || (buffer.read_size > 0 && !buffer.is_cache) {
                    is_find = true;
                    buffer.is_cache = false;
                    if ssd.tmp_file_size < buffer.size as i64 {
                        buffer.size = ssd.tmp_file_size as u64;
                    }
                }
            }
        }

        if !is_find {
            log::error!("err:!is_find");
            return Err(anyhow!("err:!is_find"));
        }
        buffer
    };

    //limit  full DataEmpty
    let ret: std::io::Result<usize> = async {
        let (sleep_read_time_millis, r, read_wait, start_size, end_size) = {
            let sss = sss.get();
            let sleep_read_time_millis = match &stream_status {
                &StreamStatus::Limit => LIMIT_SLEEP_TIME_MILLIS,
                &StreamStatus::Full => {
                    0
                }
                &StreamStatus::Ok(_) => 0,
                &StreamStatus::DataEmpty => {
                    NORMAL_SLEEP_TIME_MILLIS
                }
            };

            let start_size = buffer.read_size as usize;
            let end_size = buffer.size as usize;

            (sleep_read_time_millis, sss.r.clone(), sss.ssc.read_wait.clone(), start_size, end_size)
        };

        let mut r = r.get_mut().await;
        if r.is_read_msg().await {
            if sleep_read_time_millis > 0 {
                tokio::select! {
                    biased;
                    ret = r.read_msg(read_buffer_size) => {
                        let msg = ret?;
                        let n = msg.len();
                        buffer.push_msg(msg);
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
                let msg  = r.try_read_msg(read_buffer_size).await?;
                let n = msg.len();
                buffer.push_msg(msg);
                return Ok(n);
            }
        } else {
            if sleep_read_time_millis > 0 {
                tokio::select! {
                    biased;
                    ret = r.read(buffer.data_mut(start_size, end_size)) => {
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
            } else {
               let n = r.try_read(buffer.data_mut(start_size, end_size)).await?;
                return Ok(n);
            }
        }
    }
    .await;

    let mut sss = sss.get_mut();
    if let Err(ref e) = ret {
        //log::trace!("{}, read_err = Some(ret)", get_flag(sss.ssc.is_client));
        if e.kind() != std::io::ErrorKind::ConnectionReset {
            return Err(anyhow!("err:{}", e))?;
        }
        sss.read_err = Some(Err(anyhow!(
            "err:stream read => flag:{}, e:{}",
            get_flag(sss.ssc.is_client),
            e
        )));
    } else {
        let size = ret.unwrap() as u64;
        buffer.read_size += size;
    }
    Ok(buffer)
}
