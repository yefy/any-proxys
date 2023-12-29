use super::StreamCacheBuffer;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::StreamStreamShare;
use crate::proxy::{get_flag, StreamStatus, LIMIT_SLEEP_TIME_MILLIS};
use any_base::io::async_read_msg::AsyncReadMsgExt;
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
    pub static ref MEMORY_HANDLE_NEXT: ArcRwLockTokio<PluginHandleStream> =
        ArcRwLockTokio::default();
}

pub async fn set_memory_handle(plugin: PluginHandleStream) -> Result<()> {
    if MEMORY_HANDLE_NEXT.is_none().await {
        MEMORY_HANDLE_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn memory_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss).await
}

pub async fn handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    #[cfg(unix)]
    {
        let sendfile = sss.get().sendfile.clone();
        sendfile.set_nil().await;
    }
    let (stream_info, read_buffer_size) = {
        let mut sss = sss.get_mut();
        let stream_info = sss.stream_info.clone();
        let mut stream_info = stream_info.get_mut();
        stream_info.buffer_cache = Some(sss.ssc.cs.buffer_cache.clone());
        stream_info.add_work_time(&format!("{} memory", get_flag(sss.is_client)));

        let buffer_pool = DynamicPool::new(1, 2, StreamCacheBuffer::default);
        sss.buffer_pool.set(buffer_pool);

        let read_buffer_size = sss.ssc.cs.read_buffer_size;
        sss.is_stream_cache = false;
        (sss.stream_info.clone(), read_buffer_size)
    };

    let mut _is_first = true;
    loop {
        if sss.get().read_err.is_none() {
            let (ret, buffer) = read(sss.clone(), read_buffer_size).await?;
            let mut sss = sss.get_mut();
            if _is_first {
                _is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time(&format!("first read {}", get_flag(sss.is_client)));
            }
            sss.read_buffer = Some(buffer);
            sss.read_buffer_ret = Some(ret);
        } else {
            let mut sss = sss.get_mut();
            if sss.read_buffer.is_some() {
                if sss.read_buffer.as_ref().unwrap().read_size <= 0 {
                    log::error!("err:buffer.read_size <= 0");
                } else {
                    sss.read_buffer_ret = Some(Ok(0));
                }
            }
        }

        loop {
            check_stream_close(sss.clone()).await?;
            if sss.get().is_empty() {
                break;
            }

            let handle_next = &*MEMORY_HANDLE_NEXT.get().await;
            let stream_status = (handle_next)(sss.clone())
                .await
                .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
            if let &StreamStatus::Limit = &stream_status {
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
        }
    }
}

pub async fn check_stream_close(sss: ShareRw<StreamStreamShare>) -> Result<()> {
    let ret: Result<()> = async {
        let mut sss = sss.get_mut();
        if sss.write_err.is_some() {
            sss.write_err.take().unwrap()?;
        }

        if sss.read_err.is_some() && sss.is_read_empty() && sss.is_write_empty() {
            sss.read_err.take().unwrap()?;
        }
        Ok(())
    }
    .await;
    if ret.is_err() {
        let w = sss.get().w.clone();
        let mut w = w.get_mut().await;
        let _ = w.flush().await;
        //let _ = w.shutdown().await;
        ret?;
    }
    Ok(())
}

pub async fn read(
    sss: ShareRw<StreamStreamShare>,
    read_buffer_size: usize,
) -> Result<(std::io::Result<usize>, DynamicPoolItem<StreamCacheBuffer>)> {
    let (mut buffer, r) = {
        let sss = sss.get_mut();
        (sss.buffer_pool.get().take(), sss.r.clone())
    };

    buffer.resize(None);
    buffer.is_cache = true;

    let mut r = r.get_mut().await;

    let ret: std::io::Result<usize> = async {
        if r.is_read_msg().await {
            let msg = r.read_msg(read_buffer_size).await?;
            let n = msg.len();
            buffer.push_msg(msg);
            return Ok(n);
        } else {
            let n = r.read(buffer.data_raw()).await?;
            return Ok(n);
        }
    }
    .await;

    Ok((ret, buffer))
}
