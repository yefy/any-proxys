use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::get_flag;
use crate::proxy::{StreamStatus, StreamStreamShare};
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny, ShareRw};
use anyhow::Result;
use anyhow::{anyhow, Error};
use dynamic_pool::DynamicPoolItem;
use lazy_static::lazy_static;
use std::fs::File;
use std::io::{Read, Write};
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

pub async fn cache_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss, CACHE_HANDLE_NEXT.clone()).await
}

pub async fn memory_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<usize> {
    handle_run(sss, MEMORY_HANDLE_NEXT.clone()).await
}

pub struct StreamStreamTmpFile {
    fw_seek: u64,
    fw: ArcMutex<NamedTempFile>,
    #[cfg(unix)]
    fr_fd: i32,
    fr: ArcMutex<File>,
    is_first_buffer: bool,
}

impl StreamStreamTmpFile {
    pub fn new() -> Self {
        StreamStreamTmpFile {
            fw_seek: 0,
            fw: ArcMutex::default(),
            #[cfg(unix)]
            fr_fd: 0,
            fr: ArcMutex::default(),
            is_first_buffer: true,
        }
    }
}

pub async fn handle_run(
    sss: ShareRw<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<usize> {
    use crate::config::stream_stream_tmp_file;
    let ctx_index = stream_stream_tmp_file::module().get().ctx_index as usize;
    let plugin = sss.get().plugins[ctx_index].clone();
    let plugin = if plugin.is_none() {
        let mut _plugin = StreamStreamTmpFile::new();
        let cs = sss.get().ssc.cs.clone();
        if cs.tmp_file_size > 0 {
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

            _plugin.fw.set(fw);
            _plugin.fr.set(fr);
        }

        let plugin = ArcUnsafeAny::new(Box::new(_plugin));
        sss.get_mut().plugins[ctx_index] = Some(plugin.clone());
        plugin
    } else {
        plugin.unwrap()
    };

    write_buffer(sss.clone(), plugin.clone()).await?;
    sss.get_mut().stream_status = write(sss.clone(), handle, plugin)
        .await
        .map_err(|e| anyhow!("err:write => e:{}", e))?;
    Ok(0)
}
pub async fn write_buffer(sss: ShareRw<StreamStreamShare>, plugin: ArcUnsafeAny) -> Result<()> {
    let cs = sss.get().ssc.cs.clone();
    let ssd = sss.get().ssc.ssd.clone();
    let stream_info = sss.get().stream_info.clone();

    if sss.get_mut().read_buffer_ret.is_none() {
        return Ok(());
    }
    let read_buffer_ret = sss.get_mut().read_buffer_ret.take().unwrap();
    let mut buffer = sss.get_mut().read_buffer.take().unwrap();
    if let Err(ref e) = read_buffer_ret {
        let mut sss = sss.get_mut();
        log::trace!("{}, read_err = Some(ret)", get_flag(sss.is_client));
        if e.kind() != std::io::ErrorKind::ConnectionReset {
            return Err(anyhow!("err:{}", e))?;
        }
        sss.read_err = Some(Err(anyhow!(
            "err:stream read => flag:{}, e:{}",
            get_flag(sss.is_client),
            e
        )));
    } else {
        let mut stream_info = stream_info.get_mut();
        let mut ssd = ssd.get_mut();
        let mut sss = sss.get_mut();
        let plugin = plugin.get_mut::<StreamStreamTmpFile>();

        let size = read_buffer_ret? as u64;
        ssd.total_read_size += size;
        stream_info.total_read_size += size;
        let old_read_size = buffer.read_size;
        buffer.read_size += size;

        log::trace!(
            "{}, read size:{}, buffer.read_size:{}",
            get_flag(sss.is_client),
            size,
            buffer.read_size
        );

        if sss.read_err.is_none()
            && buffer.msg.is_none()
            && buffer.size != buffer.read_size
            && cs.is_stream_cache_merge
            && !plugin.is_first_buffer
        {
            if !sss.is_write_empty() {
                if buffer.read_size < cs.min_read_buffer_size as u64 {
                    sss.read_buffer = Some(buffer);
                    return Ok(());
                }
            } else {
                if old_read_size <= 0 && buffer.read_size < cs.min_read_buffer_size as u64 {
                    sss.read_buffer = Some(buffer);
                    return Ok(());
                }
            }
        }
        plugin.is_first_buffer = false;
    }
    if buffer.read_size == 0 {
        return Ok(());
    }

    buffer.size = buffer.read_size;
    let mut size = buffer.size;

    if buffer.is_cache {
        let mut ssd = ssd.get_mut();
        let mut sss = sss.get_mut();

        ssd.stream_cache_size -= size as i64;
        //let last_cache = sss.caches.back_mut();
        // if buffer.msg.is_none() && last_cache.is_some() {
        //     let last_cache = last_cache.unwrap();
        //     if let StreamCache::Buffer(last_cache) = last_cache {
        //         if last_cache.msg.is_none() {
        //             if last_cache.size < cs.min_merge_cache_buffer_size
        //                 || buffer.size < cs.min_merge_cache_buffer_size
        //             {
        //                 let buffer_end = buffer.size as usize;
        //                 let resize = last_cache.size as usize;
        //                 last_cache.resize(Some(resize));
        //                 last_cache
        //                     .data_raw()
        //                     .extend_from_slice(buffer.data(0, buffer_end));
        //                 last_cache.size += buffer.size;
        //                 return Ok(());
        //             }
        //         }
        //     }
        // }
        sss.caches.push_back(StreamCache::Buffer(buffer));
        return Ok(());
    }
    if buffer.msg.is_none() && cs.is_tmp_file_io_page {
        let plugin = plugin.get_mut::<StreamStreamTmpFile>();
        let mut sss = sss.get_mut();

        let next_seek_end = (plugin.fw_seek / cs.page_size as u64 + 1) * cs.page_size as u64;
        let fw_seek_end = plugin.fw_seek + size;
        let fw_seek_page = (fw_seek_end / cs.page_size as u64) * cs.page_size as u64;
        if fw_seek_end > next_seek_end {
            let left_size = fw_seek_end - fw_seek_page;
            if left_size > 0 {
                if left_size >= size {
                    log::error!(
                        "err:left_size >= size => left_size:{}, size:{}",
                        left_size,
                        size
                    );
                }
                buffer.size = size - left_size;
                buffer.read_size = buffer.size;
                let copy_start = buffer.size;
                let mut write_buffer = sss.buffer_pool.get().take();
                write_buffer
                    .data_raw()
                    .extend_from_slice(buffer.data(copy_start as usize, size as usize));
                write_buffer.read_size = left_size;
                write_buffer.resize(None);
                sss.read_buffer = Some(write_buffer);

                size = buffer.size;
            }
        }
    }

    ssd.get_mut().tmp_file_size -= size as i64;
    let fw = plugin.get_mut::<StreamStreamTmpFile>().fw.clone();
    let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
        let mut fw = fw.get_mut();
        let ret = fw
            .write_all(buffer.data(0, size as usize))
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

    if let Err(e) = ret {
        return Err(e);
    }

    #[cfg(unix)]
    let mut is_sendfile = false;
    #[cfg(unix)]
    let sendfile = sss.get().sendfile.clone();
    #[cfg(unix)]
    if sendfile.get().await.is_some() {
        is_sendfile = true;
    }
    let plugin = plugin.get_mut::<StreamStreamTmpFile>();
    let mut sss = sss.get_mut();
    if cs.is_tmp_file_io_page {
        let mut is_first = true;
        let mut size = size;
        loop {
            if size <= 0 {
                break;
            }

            let w_size = if is_first {
                cs.page_size as u64 - plugin.fw_seek % cs.page_size as u64
            } else {
                cs.page_size as u64
            };
            let w_size = if w_size > size { size } else { w_size };

            let seek = plugin.fw_seek;
            plugin.fw_seek += w_size;
            size -= w_size;

            if is_first {
                is_first = false;
                if w_size != cs.page_size as u64 {
                    let last_cache = sss.caches.back_mut();
                    if last_cache.is_some() {
                        let last_cache = last_cache.unwrap();
                        if let StreamCache::File(last_cache) = last_cache {
                            if last_cache.seek + last_cache.size != seek {
                                log::error!("err:last_cache.seek + last_cache.size != seek")
                            }
                            last_cache.size += w_size;
                            continue;
                        }
                    }
                }
            }
            let mut _file_fd = 0;
            #[cfg(unix)]
            if is_sendfile {
                _file_fd = plugin.fr_fd;
            }
            sss.caches.push_back(StreamCache::File(StreamCacheFile {
                file_fd: _file_fd,
                seek,
                size: w_size,
            }));
        }

        return Ok(());
    }

    let seek = plugin.fw_seek;
    plugin.fw_seek += size;

    let last_cache = sss.caches.back_mut();
    if last_cache.is_some() {
        let last_cache = last_cache.unwrap();
        if let StreamCache::File(last_cache) = last_cache {
            if last_cache.size < cs.min_merge_cache_buffer_size
                || size < cs.min_merge_cache_buffer_size
            {
                if last_cache.seek + last_cache.size != seek {
                    log::error!("err:last_cache.seek + last_cache.size != seek")
                }
                last_cache.size += size;
                return Ok(());
            }
        }
    }

    let mut _file_fd = 0;
    #[cfg(unix)]
    if is_sendfile {
        _file_fd = plugin.fr_fd;
    }
    sss.caches.push_back(StreamCache::File(StreamCacheFile {
        file_fd: _file_fd,
        seek,
        size,
    }));

    return Ok(());
}

pub async fn read_buffer(
    sss: ShareRw<StreamStreamShare>,
    plugin: ArcUnsafeAny,
) -> Result<Option<DynamicPoolItem<StreamCacheBuffer>>> {
    if sss.get().write_buffer.is_some() {
        log::trace!("{}, write_buffer", get_flag(sss.get().is_client));
        Ok(sss.get_mut().write_buffer.take())
    } else {
        log::trace!("{}, caches", get_flag(sss.get().is_client));
        let cache = sss.get_mut().caches.pop_front();
        if cache.is_none() {
            return Ok(None);
        }
        let cache = cache.unwrap();

        let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> = match cache {
            StreamCache::Buffer(mut buffer) => {
                buffer.is_cache = true;
                Ok(buffer)
            }
            StreamCache::File(file) => {
                let mut buffer = sss.get_mut().buffer_pool.get_mut().take();
                buffer.resize(Some(file.size as usize));
                buffer.file_fd = file.file_fd;
                buffer.size = file.size;
                buffer.seek = file.seek;

                let ret = loop {
                    #[cfg(unix)]
                    let sendfile = sss.get().sendfile.clone();
                    #[cfg(unix)]
                    if sendfile.get().await.is_some() && buffer.file_fd > 0 {
                        break Ok(buffer);
                    }

                    let fr = plugin.get::<StreamStreamTmpFile>().fr.clone();
                    let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> =
                        tokio::task::spawn_blocking(move || {
                            let end_size = buffer.size as usize;
                            let ret =
                                fr.get_mut()
                                    .read_exact(buffer.data(0, end_size))
                                    .map_err(|e| {
                                        anyhow!("err:fr.read_exact => e:{}, size:{}", e, file.size)
                                    });
                            match ret {
                                Ok(_) => return Ok(buffer),
                                Err(err) => return Err(err),
                            }
                        })
                        .await?;
                    break ret;
                };
                ret
            }
        };
        match ret {
            Ok(buffer) => Ok(Some(buffer)),
            Err(err) => return Err(err)?,
        }
    }
}

pub async fn write(
    sss: ShareRw<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
    plugin: ArcUnsafeAny,
) -> Result<StreamStatus> {
    loop {
        let buffer = read_buffer(sss.clone(), plugin.clone()).await?;
        if buffer.is_none() {
            return Ok(StreamStatus::DataEmpty);
        }
        let buffer = buffer.unwrap();
        log::trace!(
            "{}, start:{}, size:{}",
            get_flag(sss.get().is_client),
            buffer.start,
            buffer.size
        );
        sss.get_mut().write_buffer = Some(buffer);

        let handle_next = &*handle.get().await;
        (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;

        let stream_status = sss.get_mut().stream_status.clone();
        match &stream_status {
            StreamStatus::Limit => {
                return Ok(stream_status);
            }
            StreamStatus::Full(_) => {
                return Ok(stream_status);
            }
            StreamStatus::Ok(_) => {
                //
            }
            StreamStatus::DataEmpty => {
                log::error!("err:StreamStatus::DataEmpty");
            }
        }
    }
}
