use super::StreamCacheBuffer;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::{get_flag, StreamStatus, LIMIT_SLEEP_TIME_MILLIS};
use crate::proxy::{StreamCloseType, StreamStreamShare};
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
        let sendfile = sss.get().ssc.sendfile.clone();
        sendfile.set_nil().await;
    }
    let (stream_info, read_buffer_size) = {
        let mut sss = sss.get_mut();
        let stream_info = sss.stream_info.clone();
        let mut stream_info = stream_info.get_mut();
        stream_info.buffer_cache = Some(sss.ssc.cs.buffer_cache.clone());
        stream_info.add_work_time(&format!("{} memory", get_flag(sss.ssc.is_client)));

        let buffer_pool = DynamicPool::new(1, 2, StreamCacheBuffer::default);
        sss.buffer_pool.set(buffer_pool);

        let read_buffer_size = sss.ssc.cs.read_buffer_size;
        sss.is_stream_cache = false;
        (sss.stream_info.clone(), read_buffer_size)
    };

    let mut _is_first = true;
    loop {
        let buffer = read(sss.clone(), read_buffer_size).await?;
        {
            let mut sss = sss.get_mut();
            if _is_first {
                _is_first = false;
                stream_info
                    .get_mut()
                    .add_work_time(&format!("first read {}", get_flag(sss.ssc.is_client)));
            }
            sss.read_buffer = Some(buffer);
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
    Ok(())
}

pub async fn read(
    sss: ShareRw<StreamStreamShare>,
    read_buffer_size: usize,
) -> Result<DynamicPoolItem<StreamCacheBuffer>> {
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
