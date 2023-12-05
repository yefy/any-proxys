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

pub async fn memory_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss).await
}

pub async fn handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    #[cfg(unix)]
    {
        let sendfile = sss.get().sendfile.clone();
        sendfile.set_nil().await;
    }
    let stream_info = sss.get().stream_info.clone();

    stream_info.get_mut().buffer_cache = Some(sss.get().ssc.cs.buffer_cache.clone());
    stream_info
        .get_mut()
        .add_work_time(&format!("{} memory", get_flag(sss.get().is_client)));

    {
        let buffer_pool = DynamicPool::new(1, 2, StreamCacheBuffer::default);
        sss.get_mut().buffer_pool.set(buffer_pool);
    }

    let mut _is_first = true;
    loop {
        if sss.get().read_err.is_none() {
            let (ret, buffer) = read(sss.clone()).await?;
            if _is_first {
                _is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time(get_flag(sss.get().is_client));
            }
            sss.get_mut().read_buffer = Some(buffer);
            sss.get_mut().read_buffer_ret = Some(ret);
        } else {
            if sss.get().read_buffer.is_some() {
                if sss.get().read_buffer.as_ref().unwrap().read_size <= 0 {
                    log::error!("err:buffer.read_size <= 0");
                } else {
                    sss.get_mut().read_buffer_ret = Some(Ok(0));
                }
            }
        }

        loop {
            check_stream_close(sss.clone()).await?;
            if !sss.get().is_empty() {
                let handle_next = &*MEMORY_HANDLE_NEXT.get_mut().await;
                (handle_next)(sss.clone())
                    .await
                    .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
                let stream_status = sss.get().stream_status.clone();
                if let StreamStatus::Limit = stream_status {
                    tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS))
                        .await;
                }
            } else {
                break;
            }
        }
    }
}

pub async fn check_stream_close(sss: ShareRw<StreamStreamShare>) -> Result<()> {
    let mut sss = sss.get_mut();
    if sss.write_err.is_some() {
        sss.write_err.take().unwrap()?;
    }

    if sss.read_err.is_some() && sss.is_read_empty() && sss.is_write_empty() {
        sss.read_err.take().unwrap()?;
    }
    Ok(())
}

pub async fn read(
    sss: ShareRw<StreamStreamShare>,
) -> Result<(std::io::Result<usize>, DynamicPoolItem<StreamCacheBuffer>)> {
    let mut buffer = sss.get_mut().buffer_pool.get().take();
    buffer.reset();
    buffer.resize(None);
    buffer.is_cache = true;

    let r = sss.get().r.clone();
    let mut r = r.get_mut().await;

    let ret: std::io::Result<usize> = async {
        if r.is_read_msg().await {
            let msg = r.read_msg().await?;
            let n = msg.len();
            buffer.msg = Some(msg);
            return Ok(n);
        } else {
            let n = r.read(&mut buffer.data).await?;
            return Ok(n);
        }
    }
    .await;

    Ok((ret, buffer))
}
