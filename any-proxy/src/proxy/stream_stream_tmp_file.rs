use crate::config::net_core::get_tmp_file_fd;
use crate::config::net_core_plugin::PluginHandleStream;
use crate::proxy::stream_stream_buf::{
    StreamStreamBuf, StreamStreamBytes, StreamStreamCacheWrite, StreamStreamFile, StreamStreamWrite,
};
use crate::proxy::{StreamStatus, StreamStreamData, StreamStreamShare};
use any_base::file_ext::FileExt;
use any_base::io::async_write_msg::BufFile;
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny};
use anyhow::Result;
use anyhow::{anyhow, Error};
use lazy_static::lazy_static;
use std::cmp::min;
use std::io::{Read, Seek, Write};
use std::mem::swap;
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;

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
    file_ext: Arc<FileExt>,
    is_first_buffer: bool,
}

impl StreamStreamTmpFile {
    pub fn new() -> Self {
        StreamStreamTmpFile {
            fw_seek: 0,
            file_ext: FileExt::default_arc(),
            is_first_buffer: true,
        }
    }
    pub async fn file_ext(&mut self) -> Result<Arc<FileExt>> {
        if self.file_ext.file.is_none() {
            self.file_ext = get_tmp_file_fd().await?;
        }
        Ok(self.file_ext.clone())
    }
}

pub async fn handle_run(
    sss: Arc<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<StreamStatus> {
    let ssc = sss.ssc.clone();
    let plugin = {
        let (ctx_index, plugin, ssd) = {
            use crate::config::stream_stream_tmp_file;
            let ctx_index = stream_stream_tmp_file::module().get().ctx_index as usize;
            let sss_ctx = sss.sss_ctx.get_mut();
            let plugin = sss_ctx.plugins[ctx_index].clone();
            (ctx_index, plugin, sss.ssc.ssd.clone())
        };
        if plugin.is_none() {
            let mut _plugin = StreamStreamTmpFile::new();
            let plugin = ArcUnsafeAny::new(Box::new(_plugin));
            let mut sss_ctx = sss.sss_ctx.get_mut();
            sss_ctx.plugins[ctx_index] = Some(plugin.clone());
            plugin
        } else {
            let plugin = plugin.unwrap();
            if ssc.cs.max_tmp_file_size > 0 {
                if ssc.cs.tmp_file_reopen_size > 0
                    && ssd.get().tmp_file_curr_reopen_size >= ssc.cs.tmp_file_reopen_size
                {
                    ssd.get_mut().tmp_file_curr_reopen_size = 0;
                    let plugin = plugin.get_mut::<StreamStreamTmpFile>();
                    let file_ext = get_tmp_file_fd().await;
                    if file_ext.is_ok() {
                        plugin.file_ext = file_ext?;
                        plugin.fw_seek = 0;
                    }
                }
            }
            plugin
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
        ssd.tmp_file_curr_reopen_size += size;
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
    let file_ext = plugin.get_mut::<StreamStreamTmpFile>().file_ext().await?;
    if ssc.cs.is_tmp_file_io_page {
        let mut fw_seek = {
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
            let write_fw = file_ext.file.clone();
            let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
                let mut write_buffer = write_buffer.get_mut();
                let mut buf = write_buffer.buffer().unwrap();
                let mut write_fw = write_fw.get_mut();
                write_fw.seek(std::io::SeekFrom::Start(fw_seek))?;
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
            fw_seek += w_size as u64;
        }
    } else {
        let plugin = plugin.get_mut::<StreamStreamTmpFile>();
        let seek = plugin.fw_seek;
        let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
            let mut buf = buffer.buffer().unwrap();
            let fw = file_ext.file.clone();
            let fw = &mut *fw.get_mut();
            fw.seek(std::io::SeekFrom::Start(seek))?;
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
    let seek = plugin.fw_seek;
    plugin.fw_seek += size as u64;
    let file_ext = plugin.file_ext().await?;

    let mut sss_ctx = sss.sss_ctx.get_mut();
    let last_cache = sss_ctx.caches.back_mut();
    if last_cache.is_some() {
        let last_cache = last_cache.unwrap();
        if let StreamStreamWrite::File(last_cache) = &mut last_cache.data {
            if last_cache.file_ext.is_uniq_and_fd_eq(&file_ext.fix)
                && last_cache.seek + last_cache.size as u64 == seek
            {
                last_cache.size += size;
                return Ok(());
            }
        }
    }
    let buffer =
        StreamStreamCacheWrite::from_file(StreamStreamFile::new(file_ext, seek, size), false);
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
        if ssc.is_sendfile && buffer.file().unwrap().file_ext.is_sendfile() {
            let buffer = buffer.to_file().unwrap();
            return Ok(StreamStreamCacheWrite::from_file(buffer, is_cache));
        }

        let page_size = sss.page_size;
        let seek = buffer.file().unwrap().seek;
        let write_buffer_size = ssc.cs.write_buffer_size;

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
        let fr = buffer.file().unwrap().file_ext.clone();
        if buffer.remaining_() > 0 {
            sss.sss_ctx.get_mut().caches.push_front(buffer);
        }

        let ret: std::result::Result<StreamStreamCacheWrite, Error> =
            tokio::task::spawn_blocking(move || {
                #[cfg(feature = "anyio-file")]
                use std::time::Instant;
                #[cfg(feature = "anyio-file")]
                let start_time = Instant::now();
                let mut buffer = StreamStreamBytes::new();
                let mut data = vec![0u8; size];
                let fr = fr.file.clone();
                let fr = &mut *fr.get_mut();
                log::trace!(target: "main", "tmpfile seek:{}, size:{}", seek, size);
                fr.seek(std::io::SeekFrom::Start(seek))?;
                let ret = fr
                    .read_exact(data.as_mut_slice())
                    .map_err(|e| anyhow!("err:fr.read_exact => e:{}, size:{}", e, size));

                #[cfg(feature = "anyio-file")]
                if start_time.elapsed().as_millis() > 100 {
                    log::info!(
                        "read_exact file:{} => seek:{}, size:{}",
                        start_time.elapsed().as_millis(),
                        seek,
                        size
                    );
                }
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
