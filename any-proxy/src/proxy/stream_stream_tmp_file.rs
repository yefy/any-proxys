use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::get_flag;
use crate::proxy::{StreamStatus, StreamStreamShare};
use any_base::typ::{ArcMutex, ArcRwLockTokio, ArcUnsafeAny, ShareRw};
use any_base::util::StreamMsg;
use anyhow::Result;
use anyhow::{anyhow, Error};
use dynamic_pool::DynamicPoolItem;
use lazy_static::lazy_static;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::atomic::Ordering;
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

pub async fn cache_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss, CACHE_HANDLE_NEXT.clone()).await
}

pub async fn memory_handle_run(sss: ShareRw<StreamStreamShare>) -> Result<StreamStatus> {
    handle_run(sss, MEMORY_HANDLE_NEXT.clone()).await
}

pub struct StreamStreamTmpFile {
    fw_seek: u64,
    fw: Option<ArcMutex<NamedTempFile>>,
    #[cfg(unix)]
    fr_fd: i32,
    fr: Option<ArcMutex<File>>,
    is_first_buffer: bool,
}

impl StreamStreamTmpFile {
    pub fn new() -> Self {
        StreamStreamTmpFile {
            fw_seek: 0,
            fw: None,
            #[cfg(unix)]
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
    sss: ShareRw<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
) -> Result<StreamStatus> {
    let plugin = {
        let mut sss = sss.get_mut();
        use crate::config::stream_stream_tmp_file;
        let ctx_index = stream_stream_tmp_file::module().get().ctx_index as usize;
        let plugin = sss.plugins[ctx_index].clone();
        if plugin.is_none() {
            let mut _plugin = StreamStreamTmpFile::new();
            let cs = sss.ssc.cs.clone();
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

                _plugin.fw = Some(ArcMutex::new(fw));
                _plugin.fr = Some(ArcMutex::new(fr));
            }

            let plugin = ArcUnsafeAny::new(Box::new(_plugin));
            sss.plugins[ctx_index] = Some(plugin.clone());
            plugin
        } else {
            plugin.unwrap()
        }
    };

    write_buffer(sss.clone(), plugin.clone()).await?;
    write(sss.clone(), handle, plugin)
        .await
        .map_err(|e| anyhow!("err:write => e:{}", e))
}
pub async fn write_buffer(sss: ShareRw<StreamStreamShare>, plugin: ArcUnsafeAny) -> Result<()> {
    let (cs, ssd, stream_info) = {
        let sss = sss.get();
        if sss.read_buffer_ret.is_none() {
            return Ok(());
        }
        (
            sss.ssc.cs.clone(),
            sss.ssc.ssd.clone(),
            sss.stream_info.clone(),
        )
    };
    let (mut buffer, size) = {
        let mut sss = sss.get_mut();
        let mut ssd = ssd.get_mut();
        let mut stream_info = stream_info.get_mut();
        let plugin = plugin.get_mut::<StreamStreamTmpFile>();
        let read_buffer_ret = sss.read_buffer_ret.take().unwrap();
        let mut buffer = sss.read_buffer.take().unwrap();
        let is_left = if let Err(ref e) = read_buffer_ret {
            //log::trace!("{}, read_err = Some(ret)", get_flag(sss.is_client));
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                return Err(anyhow!("err:{}", e))?;
            }
            sss.read_err = Some(Err(anyhow!(
                "err:stream read => flag:{}, e:{}",
                get_flag(sss.is_client),
                e
            )));
            false
        } else {
            let size = read_buffer_ret? as u64;
            ssd.total_read_size += size;
            stream_info.total_read_size += size;
            buffer.read_size += size;

            // log::trace!(
            //     "{}, read size:{}, buffer.read_size:{}",
            //     get_flag(sss.is_client),
            //     size,
            //     buffer.read_size
            // );

            let is_left = if buffer.is_cache {
                if cs.max_stream_cache_size > 0 {
                    ssd.stream_cache_size -= size as i64;
                    if ssd.stream_cache_size > 0 {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                if cs.max_tmp_file_size > 0 {
                    ssd.tmp_file_size -= size as i64;
                    if ssd.tmp_file_size > 0 {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if is_left
                && cs.is_open_stream_cache_merge
                && sss.read_err.is_none()
                && sss.is_stream_cache
                && buffer.read_size < buffer.size
                && !plugin.is_first_buffer
            {
                if buffer.read_size < cs.min_read_buffer_size as u64 {
                    if !sss.ssc.delay_ok.load(Ordering::SeqCst) {
                        if sss.is_write_empty() {
                            sss.delay_start();
                        }

                        ssd.total_read_size -= size;
                        stream_info.total_read_size -= size;
                        if buffer.is_cache {
                            ssd.stream_cache_size += size as i64;
                        } else {
                            ssd.tmp_file_size += size as i64;
                        }
                        sss.read_buffer = Some(buffer);
                        return Ok(());
                    }
                }
            }
            is_left
        };

        if buffer.read_size == 0 {
            return Ok(());
        }

        let mut size = buffer.read_size;
        {
            if buffer.is_cache {
                sss.delay_stop();
                if plugin.is_first_buffer {
                    plugin.is_first_buffer = false;
                }
                if sss.caches.len() > 0 || sss.write_buffer.is_some() {
                    sss.caches.push_back(StreamCache::Buffer(buffer));
                } else {
                    sss.write_buffer = Some(buffer);
                }
                return Ok(());
            }

            if is_left
                && cs.is_tmp_file_io_page
                && sss.read_err.is_none()
                && sss.is_stream_cache
                && buffer.read_size < buffer.size
                && !plugin.is_first_buffer
            {
                let fw_seek_end = plugin.fw_seek + size;
                let fw_seek_left = fw_seek_end % cs.page_size as u64;
                if fw_seek_left > 0 {
                    let fw_seek_end_next_page =
                        (fw_seek_end / cs.page_size as u64 + 1) * cs.page_size as u64;
                    if fw_seek_end < fw_seek_end_next_page {
                        if !sss.ssc.delay_ok.load(Ordering::SeqCst) {
                            if sss.is_write_empty() {
                                sss.delay_start();
                            }

                            sss.read_buffer = Some(buffer);
                            ssd.total_read_size -= size;
                            stream_info.total_read_size -= size;
                            ssd.tmp_file_size += size as i64;
                            return Ok(());
                        }
                    } else {
                        let left_size = fw_seek_left;

                        buffer.read_size = size - left_size;
                        let copy_start = buffer.read_size as usize;
                        let copy_end = size as usize;
                        size = buffer.read_size;

                        let mut write_buffer = sss.buffer_pool.get().take();
                        write_buffer.is_cache = false;
                        write_buffer.resize_ext(left_size as usize);
                        if buffer.msg.is_some() {
                            let mut msg = StreamMsg::new();
                            let datas = buffer.data_or_msg(copy_start, copy_end).to_vec();
                            msg.push(datas);
                            write_buffer.push_msg(msg);
                        } else {
                            write_buffer.set_len(0);
                            write_buffer
                                .data_raw()
                                .extend_from_slice(buffer.data_or_msg(copy_start, copy_end));
                            write_buffer.resize_curr_size();
                        }
                        write_buffer.read_size = left_size;
                        sss.read_buffer = Some(write_buffer);

                        ssd.total_read_size -= left_size;
                        stream_info.total_read_size -= left_size;
                        ssd.tmp_file_size += left_size as i64;
                    }
                }
            }

            sss.delay_stop();
            if plugin.is_first_buffer {
                plugin.is_first_buffer = false;
            }
        }
        (buffer, size)
    };

    let fw = plugin.get::<StreamStreamTmpFile>().fw();
    let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
        let mut fw = fw.get_mut();
        let ret = fw
            .write_all(buffer.data_or_msg(0, size as usize))
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
            if last_cache.size < cs.page_size as u64 || size < cs.page_size as u64 {
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
) -> Result<StreamStatus> {
    if sss.get().write_buffer.is_some() {
        return Ok(StreamStatus::Full);
    }

    //log::trace!("{}, caches", get_flag(sss.get().is_client));
    let cache = sss.get_mut().caches.pop_front();
    if cache.is_none() {
        return Ok(StreamStatus::DataEmpty);
    }
    let cache = cache.unwrap();

    let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> = match cache {
        StreamCache::Buffer(mut buffer) => {
            buffer.is_cache = true;
            Ok(buffer)
        }
        StreamCache::File(file) => {
            let mut buffer = sss.get_mut().buffer_pool.get_mut().take();
            buffer.is_cache = false;
            buffer.resize(Some(file.size as usize));
            buffer.read_size = file.size;
            buffer.file_fd = file.file_fd;
            buffer.seek = file.seek;

            let ret = loop {
                #[cfg(unix)]
                let sendfile = sss.get().sendfile.clone();
                #[cfg(unix)]
                if sendfile.get().await.is_some() && buffer.file_fd > 0 {
                    break Ok(buffer);
                }

                let fr = plugin.get::<StreamStreamTmpFile>().fr();
                let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> =
                    tokio::task::spawn_blocking(move || {
                        let end_size = buffer.read_size as usize;
                        let ret = fr
                            .get_mut()
                            .read_exact(buffer.data_mut(0, end_size))
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
        Ok(buffer) => {
            sss.get_mut().write_buffer = Some(buffer);
            return Ok(StreamStatus::Full);
        }
        Err(err) => return Err(err)?,
    }
}

//limit  full DataEmpty
pub async fn write(
    sss: ShareRw<StreamStreamShare>,
    handle: ArcRwLockTokio<PluginHandleStream>,
    plugin: ArcUnsafeAny,
) -> Result<StreamStatus> {
    loop {
        let status = read_buffer(sss.clone(), plugin.clone()).await?;
        if let &StreamStatus::DataEmpty = &status {
            return Ok(status);
        }

        let handle_next = &*handle.get().await;
        let stream_status = (handle_next)(sss.clone())
            .await
            .map_err(|e| anyhow!("err:handle => e:{}", e))?;

        match &stream_status {
            &StreamStatus::Limit => {
                return Ok(stream_status);
            }
            &StreamStatus::Full => {
                return Ok(stream_status);
            }
            &StreamStatus::Ok(_) => {
                //
            }
            &StreamStatus::DataEmpty => {
                log::error!("err:StreamStatus::DataEmpty from write");
                return Ok(stream_status);
            }
        }
    }
}
