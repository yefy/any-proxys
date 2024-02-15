use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::stream_stream_buf::{
    StreamStreamBuf, StreamStreamBytes, StreamStreamCacheWrite, StreamStreamFile, StreamStreamWrite,
};
use crate::proxy::{StreamStatus, StreamStreamData, StreamStreamShare};
use any_base::io::async_write_msg::BufFile;
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny};
use anyhow::Result;
use anyhow::{anyhow, Error};
use lazy_static::lazy_static;
use std::cmp::min;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::swap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tempfile::Builder;
use tempfile::NamedTempFile;

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

pub struct StreamStreamTmpFile {
    fw_seek: u64,
    fw: Option<ArcMutex<NamedTempFile>>,
    fr_fd: i32,
    fr: Option<ArcMutex<File>>,
    is_first_buffer: bool,
}

impl StreamStreamTmpFile {
    pub fn new() -> Self {
        StreamStreamTmpFile {
            fw_seek: 0,
            fw: None,
            fr_fd: 0,
            fr: None,
            is_first_buffer: true,
        }
    }
    pub fn fw(&self) -> ArcMutex<NamedTempFile> {
        self.fw.clone().unwrap()
    }
    pub fn fr(&self) -> ArcMutex<File> {
        self.fr.clone().unwrap()
    }
}

pub async fn handle_run(
    sss: Arc<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let plugin = {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        use crate::config::stream_stream_tmp_file;
        let ctx_index = stream_stream_tmp_file::module().get().ctx_index as usize;
        let plugin = sss_ctx.plugins[ctx_index].clone();
        if plugin.is_none() {
            let mut _plugin = StreamStreamTmpFile::new();
            if ssc.cs.tmp_file_size > 0 {
                let fw = Builder::new()
                    .append(true)
                    .tempfile()
                    .map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
                //NamedTempFile::new().map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
                let fr = fw
                    .reopen()
                    .map_err(|e| anyhow!("err:fw.reopen => e:{}", e))?;

                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    _plugin.fr_fd = fr.as_raw_fd();
                }

                _plugin.fw = Some(ArcMutex::new(fw));
                _plugin.fr = Some(ArcMutex::new(fr));
            }

            let plugin = ArcUnsafeAny::new(Box::new(_plugin));
            sss_ctx.plugins[ctx_index] = Some(plugin.clone());
            plugin
        } else {
            plugin.unwrap()
        }
    };

    write_buffer(&sss, plugin).await?;
    write(sss, handle)
        .await
        .map_err(|e| anyhow!("err:write => e:{}", e))
}

pub fn change_stream_size(is_cache: bool, size: i64, ssd: &mut StreamStreamData) {
    ssd.total_read_size += size as u64;
    if ssd.stream_nodelay_size > 0 {
        ssd.stream_nodelay_size -= size;
    }
    if is_cache {
        ssd.stream_cache_size -= size;
    } else {
        ssd.tmp_file_size -= size;
    }
}

pub fn is_left(is_cache: bool, size: usize, ssd: &mut StreamStreamData) -> bool {
    let cache_size = if is_cache {
        ssd.stream_cache_size
    } else {
        ssd.tmp_file_size
    };
    cache_size > size as i64
}

pub async fn write_buffer(sss: &StreamStreamShare, plugin: ArcUnsafeAny) -> Result<()> {
    let ssc = sss.ssc.clone();
    let ssd = sss.ssc.ssd.clone();
    let stream_info = sss.stream_info.clone();

    if sss.sss_ctx.get_mut().read_buffer.is_none() {
        return Ok(());
    }

    let (mut buffer, size) = {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        let mut ssd = ssc.ssd.get_mut();
        let plugin = plugin.get_mut::<StreamStreamTmpFile>();

        let mut buffer = sss_ctx.read_buffer.take().unwrap();
        let mut size = buffer.remaining_();
        if size <= 0 {
            if !buffer.is_flush {
                sss_ctx.read_buffer = Some(buffer);
            }
            return Ok(());
        }
        let is_cache = buffer.is_cache;
        let is_left = is_left(is_cache, size, &mut ssd);
        if is_left
            && sss_ctx.read_err.is_none()
            && sss_ctx.is_stream_cache
            && !buffer.is_full()
            && !plugin.is_first_buffer
            && !buffer.is_flush
            && ssc.cs.stream_delay_mil_time > 0
            && ssd.stream_nodelay_size <= 0
        {
            if size < ssc.cs.min_read_buffer_size {
                if !sss.delay_is_ok() {
                    sss.delay_start();
                    sss_ctx.read_buffer = Some(buffer);
                    return Ok(());
                }
            }
        }

        if is_cache || buffer.is_file() {
            sss.delay_stop();
            if plugin.is_first_buffer {
                stream_info
                    .get_mut()
                    .add_work_time2(ssc.is_client, "first to cache");
                plugin.is_first_buffer = false;
            }
            sss_ctx.caches.push_back(buffer.to_cache_write());
            change_stream_size(is_cache, size as i64, &mut ssd);
            return Ok(());
        }

        let mut is_stop = true;
        if is_left
            && ssc.cs.is_tmp_file_io_page
            && sss_ctx.read_err.is_none()
            && sss_ctx.is_stream_cache
            && !buffer.is_full()
            && !plugin.is_first_buffer
            && !buffer.is_flush
            && ssc.cs.stream_delay_mil_time > 0
            && ssd.stream_nodelay_size <= 0
        {
            let fw_seek_end = plugin.fw_seek + size as u64;
            let fw_seek_left = fw_seek_end % ssc.cs.page_size as u64;
            if fw_seek_left > 0 {
                let fw_seek_next_page =
                    (plugin.fw_seek / ssc.cs.page_size as u64 + 1) * ssc.cs.page_size as u64;
                if fw_seek_end < fw_seek_next_page {
                    if !sss.delay_is_ok() {
                        sss.delay_start();
                        sss_ctx.read_buffer = Some(buffer);
                        return Ok(());
                    }
                } else {
                    let mut buf = buffer.buffer().unwrap();
                    let left_size = fw_seek_left as usize;
                    let mut rollbak_buffer = buf.split_to((size - left_size) as usize);
                    swap(&mut rollbak_buffer, &mut buffer);
                    size = buffer.remaining_();

                    rollbak_buffer.resize_max_size(ssc.cs.read_buffer_size);

                    sss_ctx.read_buffer = Some(rollbak_buffer);
                    sss.delay_start();
                    is_stop = false;
                }
            }
        }

        if is_stop {
            sss.delay_stop();
        }
        if plugin.is_first_buffer {
            stream_info
                .get_mut()
                .add_work_time2(ssc.is_client, "first to file");
            plugin.is_first_buffer = false;
        }

        (buffer, size)
    };

    change_stream_size(buffer.is_cache, size as i64, &mut ssd.get_mut());
    let fw = plugin.get::<StreamStreamTmpFile>().fw();
    if ssc.cs.is_tmp_file_io_page {
        let fw_seek = {
            let plugin = plugin.get_mut::<StreamStreamTmpFile>();
            plugin.fw_seek
        };
        let buffer = ArcMutex::new(buffer);
        let mut is_first = true;
        let mut size = size;
        loop {
            if size <= 0 {
                break;
            }

            let w_size = if is_first {
                is_first = false;
                ssc.cs.write_buffer_size - (fw_seek % ssc.cs.write_buffer_size as u64) as usize
            } else {
                ssc.cs.write_buffer_size
            };
            let w_size = min(w_size, size);
            size -= w_size;

            let write_buffer = buffer.clone();
            let write_fw = fw.clone();
            let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
                let mut write_buffer = write_buffer.get_mut();
                let mut buf = write_buffer.buffer().unwrap();
                let mut write_fw = write_fw.get_mut();
                let ret = write_fw
                    .write_all(buf.chunk(w_size))
                    .map_err(|e| anyhow!("err:fw.write_all => e:{}, w_size:{}", e, w_size));
                match ret {
                    Ok(_) => {
                        if let Err(e) = write_fw.flush() {
                            return Err(anyhow!("{}", e));
                        }
                        buf.advance(w_size);
                        Ok(())
                    }
                    Err(e) => Err(anyhow!("{}", e)),
                }
            })
            .await?;
            ret?;
        }
    } else {
        let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
            let mut buf = buffer.buffer().unwrap();
            let mut fw = fw.get_mut();
            let ret = fw
                .write_all(buf.chunk(size))
                .map_err(|e| anyhow!("err:fw.write_all => e:{}, size:{}", e, size));
            match ret {
                Ok(_) => {
                    if let Err(e) = fw.flush() {
                        return Err(anyhow!("{}", e));
                    }
                    Ok(())
                }
                Err(e) => Err(anyhow!("{}", e)),
            }
        })
        .await?;
        ret?;
    }

    let plugin = plugin.get_mut::<StreamStreamTmpFile>();
    let mut sss_ctx = sss.sss_ctx.get_mut();
    let seek = plugin.fw_seek;
    plugin.fw_seek += size as u64;

    let last_cache = sss_ctx.caches.back_mut();
    if last_cache.is_some() {
        let last_cache = last_cache.unwrap();
        if let StreamStreamWrite::File(last_cache) = &mut last_cache.data {
            if last_cache.file_fd == plugin.fr_fd
                && last_cache.seek + last_cache.size as u64 == seek
            {
                last_cache.size += size;
                return Ok(());
            }
        }
    }
    let buffer = StreamStreamCacheWrite::from_file(
        StreamStreamFile::new(plugin.fr(), plugin.fr_fd, seek, size),
        false,
    );
    sss_ctx.caches.push_back(buffer);

    return Ok(());
}

pub async fn read_buffer(sss: &StreamStreamShare) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let mut buffer = {
        let mut sss_ctx = sss.sss_ctx.get_mut();
        if sss_ctx.write_buffer.is_some() {
            return Ok(StreamStatus::Full(false));
        }
        if sss_ctx.caches.len() <= 0 {
            return Ok(StreamStatus::WriteEmpty);
        }
        sss_ctx.caches.pop_front().unwrap()
    };
    let is_cache = buffer.is_cache;

    let ret: std::result::Result<StreamStreamCacheWrite, Error> = async {
        if !buffer.is_file() {
            let buffer = buffer.to_bytes().unwrap();
            return Ok(StreamStreamCacheWrite::from_bytes(buffer, is_cache));
        }
        #[cfg(unix)]
        if ssc.is_sendfile && !sss.is_write_msg {
            let buffer = buffer.to_file().unwrap();
            return Ok(StreamStreamCacheWrite::from_file(buffer, is_cache));
        }

        use crate::util::default_config::PAGE_SIZE;
        let page_size = PAGE_SIZE.load(Ordering::Relaxed);
        let seek = buffer.file().unwrap().seek;
        let mut write_buffer_size = ssc.cs.write_buffer_size;
        if ssc.is_sendfile {
            write_buffer_size = if ssc.cs.sendfile_max_write_size <= 0 {
                usize::max_value()
            } else {
                ssc.cs.sendfile_max_write_size
            }
        }

        let write_buffer_size = min(buffer.remaining_(), write_buffer_size);
        let write_buffer_size = if write_buffer_size <= page_size {
            write_buffer_size
        } else {
            write_buffer_size / page_size * page_size
        };
        let left_seek = page_size - (seek % page_size as u64) as usize;
        let size = if write_buffer_size <= left_seek {
            write_buffer_size
        } else {
            left_seek + (write_buffer_size - left_seek) / page_size * page_size
        };

        buffer.advance_(size);
        let (fr, _file_fd) = {
            let file = buffer.file().unwrap();
            (file.fr.clone(), file.file_fd)
        };
        if buffer.remaining_() > 0 {
            sss.sss_ctx.get_mut().caches.push_front(buffer);
        }

        #[cfg(unix)]
        if ssc.is_sendfile {
            let buffer = StreamStreamFile::new(fr, _file_fd, seek, size);
            return Ok(StreamStreamCacheWrite::from_file(buffer, is_cache));
        }

        let ret: std::result::Result<StreamStreamCacheWrite, Error> =
            tokio::task::spawn_blocking(move || {
                let mut buffer = StreamStreamBytes::new();
                let mut data = vec![0u8; size];
                let ret = fr
                    .get_mut()
                    .read_exact(data.as_mut_slice())
                    .map_err(|e| anyhow!("err:fr.read_exact => e:{}, size:{}", e, size));
                match ret {
                    Ok(_) => {
                        buffer.push_back(data.into());
                        return Ok(StreamStreamCacheWrite::from_bytes(buffer, is_cache));
                    }
                    Err(err) => return Err(err),
                }
            })
            .await?;

        Ok(ret?)
    }
    .await;

    match ret {
        Ok(buffer) => {
            sss.sss_ctx.get_mut().write_buffer = Some(buffer);
            return Ok(StreamStatus::Full(false));
        }
        Err(err) => return Err(err)?,
    }
}

//limit  full DataEmpty
pub async fn write(
    sss: Arc<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<StreamStatus> {
    loop {
        let _status = read_buffer(&sss).await?;

        let handle_next = &*handle.get().await;
        let stream_status = (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:handle => e:{}", e))?;

        match &stream_status {
            &StreamStatus::Limit => {
                return Ok(stream_status);
            }
            &StreamStatus::Full(_) => {
                return Ok(stream_status);
            }
            &StreamStatus::Ok => {
                //
            }
            &StreamStatus::WriteEmpty => {
                return Ok(stream_status);
            }
        }
    }
}
