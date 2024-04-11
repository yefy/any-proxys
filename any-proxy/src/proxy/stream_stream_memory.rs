use crate::config::net_core_plugin::PluginHandleStream;
use crate::proxy::stream_stream_buf::{
    StreamStreamBuffer, StreamStreamBytes, StreamStreamCacheRead,
};
use crate::proxy::{StreamCloseType, StreamStreamShare};
use crate::proxy::{StreamStatus, LIMIT_SLEEP_TIME_MILLIS};
use any_base::io::async_read_msg::AsyncReadMsg;
use any_base::io::async_read_msg::AsyncReadMsgExt;
use any_base::typ::ArcRwLockTokio;
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
    pub static ref MEMORY_HANDLE_NEXT: ArcRwLockTokio<PluginHandleStream> =
        ArcRwLockTokio::default();
}

pub async fn set_memory_handle(plugin: PluginHandleStream) -> Result<()> {
    if MEMORY_HANDLE_NEXT.is_none().await {
        MEMORY_HANDLE_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn memory_handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss).await
}

pub async fn handle_run(sss: Arc<StreamStreamShare>) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let stream_info = sss.stream_info.clone();
    {
        let mut stream_info = stream_info.get_mut();
        stream_info.buffer_cache = Some(ssc.cs.buffer_cache.clone());
        stream_info.add_work_time2(ssc.is_client, "memory");
        sss.sss_ctx.get_mut().is_stream_cache = false;
    }

    let mut _is_first = true;
    loop {
        check_stream_close(&sss).await?;
        if _is_first {
            _is_first = false;
            stream_info
                .get_mut()
                .add_work_time2(ssc.is_client, "first read");
        }

        let buffers = read(&sss).await?;
        for buffer in buffers {
            sss.sss_ctx.get_mut().read_buffer = Some(buffer);
            loop {
                check_stream_close(&sss).await?;
                if sss.is_empty(&sss.sss_ctx.get()) {
                    break;
                }

                let handle_next = &*MEMORY_HANDLE_NEXT.get().await;
                let stream_status = (handle_next)(sss.clone())
                    .await
                    .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
                if let &StreamStatus::Limit = &stream_status {
                    tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS))
                        .await;
                }
            }
        }
    }
}

pub async fn check_stream_close(sss: &StreamStreamShare) -> Result<()> {
    let ssc = sss.ssc.clone();
    let stream_info = sss.stream_info.clone();

    let ret: Result<()> = async {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        if sss_ctx.write_err.is_some() {
            sss_ctx.write_err.take().unwrap()?;
        }

        if sss_ctx.read_err.is_some() && sss.is_read_empty(&sss_ctx) && sss.is_write_empty(&sss_ctx)
        {
            sss_ctx.read_err.take().unwrap()?;
        }
        Ok(())
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

        stream_info.get_mut().add_work_time2(ssc.is_client, "close");
        ret?;
    }
    Ok(())
}

pub async fn read(sss: &StreamStreamShare) -> Result<VecDeque<StreamStreamCacheRead>> {
    let ssc = sss.ssc.clone();
    let mut r = sss.r.get_mut().await;

    let mut buffer = if r.is_read_msg() {
        StreamStreamCacheRead::from_bytes(StreamStreamBytes::new())
    } else {
        StreamStreamCacheRead::from_buffer(StreamStreamBuffer::new(ssc.cs.read_buffer_size))
    };
    buffer.is_cache = true;
    buffer.is_flush = true;

    let ret: std::io::Result<Option<StreamStreamCacheRead>> = async {
        if r.is_read_msg() {
            let msg = r.read_msg(ssc.cs.read_buffer_size).await?;
            return Ok(buffer.push_back_ream_msg(msg));
        }

        let buf = buffer.buf().unwrap();
        let n = r.read(buf.free_data_mut()).await?;
        buffer.add_size(n);
        Ok(None)
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
