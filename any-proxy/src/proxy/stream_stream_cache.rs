use crate::config::net_core_plugin::PluginHandleStream;
use crate::proxy::stream_stream_buf::{
    StreamStreamBuffer, StreamStreamBytes, StreamStreamCacheRead,
};
use crate::proxy::{StreamCloseType, StreamStatus, SENDFILE_EAGAIN_TIME_MILLIS};
use crate::proxy::{StreamStreamShare, LIMIT_SLEEP_TIME_MILLIS, NORMAL_SLEEP_TIME_MILLIS};
use any_base::io::async_read_msg::AsyncReadMsg;
use any_base::io::async_read_msg::AsyncReadMsgExt;
use any_base::io::async_stream::AsyncStreamExt;
use any_base::io::async_write_msg::BufFile;
use any_base::typ::ArcRwLockTokio;
use any_base::util::StreamReadMsg;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::sync::Arc;
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

pub async fn cache_handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss).await
}

pub async fn handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let stream_info = sss.stream_info.clone();

    {
        let mut stream_info = stream_info.get_mut();
        stream_info.buffer_cache = Some(ssc.cs.buffer_cache.clone());
        stream_info.add_work_time2(ssc.is_client, "cache");
    }
    sss.sss_ctx.get_mut().is_stream_cache = true;

    let mut is_first = true;
    let mut stream_status = StreamStatus::WriteEmpty;

    loop {
        let is_read = {
            let ssd = ssc.ssd.get();
            let sss_ctx = sss.sss_ctx.get();
            (ssd.stream_cache_size > 0 || ssd.tmp_file_size > 0) && sss_ctx.read_err.is_none()
        };
        if is_read {
            let buffers = read(&sss, &stream_status).await?;
            if is_first {
                is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time2(ssc.is_client, "first read");
            }
            if buffers.len() > 0 {
                for buffer in buffers {
                    sss.sss_ctx.get_mut().read_buffer = Some(buffer);
                    let handle_next = &*CACHE_HANDLE_NEXT.get().await;
                    stream_status = (handle_next)(sss.clone())
                        .await
                        .map_err(|e| anyhow!("err:CACHE_HANDLE_NEXT => e:{}", e))?;

                    check_stream_close(&sss, &stream_status).await?;
                }
                continue;
            }
        } else {
            StreamStreamShare::stream_status_sleep(&stream_status, ssc.is_client).await;
        }

        let handle_next = &*CACHE_HANDLE_NEXT.get().await;
        stream_status = (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:CACHE_HANDLE_NEXT => e:{}", e))?;

        check_stream_close(&sss, &stream_status).await?;
    }
}

pub async fn check_stream_close(
    sss: &StreamStreamShare,
    stream_status: &StreamStatus,
) -> Result<()> {
    let ssc = sss.ssc.clone();
    let stream_info = sss.stream_info.clone();

    let is_single = sss.r.get_mut().await.is_single().await;
    let is_sendfile_close = StreamStreamShare::is_sendfile_close(sss, stream_status).await;

    let ret: Result<bool> = async {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        if let &StreamCloseType::WaitEmpty = &ssc.close_type {
            return Ok(true);
        }
        if sss_ctx.write_err.is_some() {
            sss_ctx.write_err.take().unwrap()?;
        }

        if sss_ctx.read_err.is_some() && sss.is_read_empty(&sss_ctx) && sss.is_write_empty(&sss_ctx)
        {
            sss_ctx.read_err.take().unwrap()?;
        }
        Ok(false)
    }
    .await;
    if ret.is_err() {
        let mut w = sss.w.get_mut().await;
        let _ = w.flush().await;
        if let &StreamCloseType::Shutdown = &ssc.close_type {
            let _ = w.shutdown().await;
            ssc.worker_inner.done();
            let _ = ssc.wait_group.wait().await;
        }
        stream_info
            .get_mut()
            .add_work_time2(ssc.is_client, "close1");
        ret?;
        return Ok(());
    }

    let is_wait_empty = ret.unwrap();
    if !is_wait_empty {
        return Ok(());
    }

    let is_close = {
        let sss_ctx = sss.sss_ctx.get_mut();
        sss_ctx.write_err.is_some()
            || (sss_ctx.read_err.is_some()
                && sss.is_read_empty(&sss_ctx)
                && sss.is_write_empty(&sss_ctx))
            || (!is_single
                && stream_info.get().close_num >= 1
                && sss.is_read_empty(&sss_ctx)
                && sss.is_write_empty(&sss_ctx))
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
                let mut sss_ctx = sss.sss_ctx.get_mut();
                if sss_ctx.write_err.is_some() {
                    stream_info
                        .get_mut()
                        .add_work_time2(ssc.is_client, "close2");
                    sss_ctx.write_err.take().unwrap()?;
                }

                if sss_ctx.read_err.is_some() {
                    stream_info
                        .get_mut()
                        .add_work_time2(ssc.is_client, "close2");
                    sss_ctx.read_err.take().unwrap()?;
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
    sss: &StreamStreamShare,
    stream_status: &StreamStatus,
) -> Result<VecDeque<StreamStreamCacheRead>> {
    let ssc = sss.ssc.clone();
    let mut r = sss.r.get_mut().await;

    let (mut buffer, left) = {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        let ssd = ssc.ssd.get();
        let mut buffer = if sss_ctx.read_buffer.is_some() {
            sss_ctx.read_buffer.take().unwrap()
        } else {
            if r.is_read_msg() {
                StreamStreamCacheRead::from_bytes(StreamStreamBytes::new())
            } else {
                StreamStreamCacheRead::from_buffer(StreamStreamBuffer::new(ssc.cs.read_buffer_size))
            }
        };

        let read_size = buffer.remaining_() as i64;
        let mut max_size = buffer.max_size() as i64;
        if max_size <= 0 {
            max_size = ssc.cs.read_buffer_size as i64;
        }
        if ssd.stream_cache_size <= read_size && ssd.tmp_file_size <= read_size {
            log::error!("err:buffer.read_size");
            return Err(anyhow!("err:buffer.read_size"));
        }
        let mut is_find = false;
        if ssd.stream_cache_size > 0 && ssd.stream_cache_size > read_size {
            if read_size == 0 || (read_size > 0 && buffer.is_cache) {
                is_find = true;
                buffer.is_cache = true;
                if ssd.stream_cache_size < max_size {
                    max_size = ssd.stream_cache_size;
                    buffer.resize_max_size(max_size as usize);
                }
            }
        }

        if !is_find {
            if ssd.tmp_file_size > 0 && ssd.tmp_file_size > read_size as i64 {
                if read_size == 0 || (read_size > 0 && !buffer.is_cache) {
                    is_find = true;
                    buffer.is_cache = false;
                    if ssd.tmp_file_size < max_size {
                        max_size = ssd.tmp_file_size;
                        buffer.resize_max_size(max_size as usize);
                    }
                }
            }
        }

        if !is_find {
            log::error!("err:!is_find");
            return Err(anyhow!("err:!is_find"));
        }
        (buffer, max_size - read_size)
    };

    let sleep_read_time_millis = {
        match &stream_status {
            &StreamStatus::Limit => LIMIT_SLEEP_TIME_MILLIS,
            &StreamStatus::Full(is_sendfile) => {
                tokio::task::yield_now().await;
                if *is_sendfile {
                    SENDFILE_EAGAIN_TIME_MILLIS
                } else {
                    0
                }
            }
            &StreamStatus::Ok => 0,
            &StreamStatus::WriteEmpty => NORMAL_SLEEP_TIME_MILLIS,
        }
    };

    //limit  full DataEmpty
    let ret: std::io::Result<Option<StreamStreamCacheRead>> = async {
        if r.is_read_msg() {
            let msg: StreamReadMsg = if sleep_read_time_millis > 0 {
                tokio::select! {
                    biased;
                    ret = r.read_msg(left as usize) => {
                        ret?
                    },
                    _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                        return Ok(None);
                    },
                     _= ssc.read_wait.clone() => {
                        return Ok(None);
                    },
                    else => {
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                        "err:do_stream_to_stream_or_file select close"
                    )));
                    }
                }
            } else {
                r.try_read_msg(left as usize).await?
            };

            return Ok(buffer.push_back_ream_msg(msg));
        }

        let buf = buffer.buf().unwrap();
        let n = if sleep_read_time_millis > 0 {
            tokio::select! {
                biased;
                ret = r.read(buf.free_data_mut()) => {
                   ret?
                },
                _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                    return Ok(None);
                },
                 _= ssc.read_wait.clone() => {
                    return Ok(None);
                },
                else => {
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                    "err:do_stream_to_stream_or_file select close"
                )));
                }
            }
        } else {
            r.try_read(buf.free_data_mut()).await?
        };
        buffer.add_size(n);
        return Ok(None);
    }
    .await;

    if let Err(ref e) = ret {
        if e.kind() != std::io::ErrorKind::ConnectionReset {
            return Err(anyhow!("err:{}", e))?;
        }
        sss.sss_ctx.get_mut().read_err = Some(Err(anyhow!("err:stream read => e:{}", e)));
        return Ok(VecDeque::from([buffer]));
    }

    let ret = ret.unwrap();
    if ret.is_none() {
        Ok(VecDeque::from([buffer]))
    } else {
        Ok(VecDeque::from([buffer, ret.unwrap()]))
    }
}
