#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info::StreamInfo;
use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use super::StreamConfigContext;
use super::StreamLimit;
use super::StreamStreamContext;
use crate::proxy::stream_stream::{StreamFullInfo, StreamStatus, StreamStream};
use crate::util::default_config;
use any_base::io::async_read_msg::{AsyncReadMsg, AsyncReadMsgExt};
use any_base::io::async_stream::{AsyncStream, AsyncStreamExt};
use any_base::io::async_write_msg::AsyncWriteMsg;
use anyhow::Result;
use anyhow::{anyhow, Error};
use dynamic_pool::{DynamicPool, DynamicPoolItem};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tempfile::Builder;
use tempfile::NamedTempFile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

const LIMIT_SLEEP_TIME_MILLIS: u64 = 100;
const NORMAL_SLEEP_TIME_MILLIS: u64 = 1000 * 1;
const NOT_SLEEP_TIME_MILLIS: u64 = 2;
const MIN_SLEEP_TIME_MILLIS: u64 = 10;
const SENDFILE_FULL_SLEEP_TIME_MILLIS: u64 = 30;

pub struct StreamStreamCacheOrFile<
    R: AsyncRead + AsyncReadMsg + AsyncStream + Unpin,
    W: AsyncWrite + AsyncWriteMsg + Unpin,
> {
    left_buffer: Option<DynamicPoolItem<StreamCacheBuffer>>,
    read_buffer: Option<DynamicPoolItem<StreamCacheBuffer>>,
    read_err: Option<Result<()>>,
    stream_status: StreamStatus,
    caches: VecDeque<StreamCache>,
    buffer_pool: DynamicPool<StreamCacheBuffer>,
    stream_info: Rc<RefCell<StreamInfo>>,
    r: R,
    w: W,
    #[cfg(unix)]
    sendfile: Option<SendFile>,
    is_client: bool,
    fw_seek: u64,
    fw: Arc<Mutex<NamedTempFile>>,
    #[cfg(unix)]
    fr_fd: i32,
    fr: Arc<Mutex<File>>,
    write_err: Option<Result<()>>,
}

impl<R: AsyncRead + AsyncReadMsg + AsyncStream + Unpin, W: AsyncWrite + AsyncWriteMsg + Unpin>
    StreamStreamCacheOrFile<R, W>
{
    pub fn new(
        stream_info: Rc<RefCell<StreamInfo>>,
        r: R,
        w: W,
        #[cfg(unix)] sendfile: Option<SendFile>,
        is_client: bool,
        stream_cache_size: usize,
    ) -> Result<StreamStreamCacheOrFile<R, W>> {
        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let maximum_capacity = std::cmp::max(stream_cache_size as u64, page_size as u64 * 32);

        let initial_capacity = 8;
        let maximum_capacity = (maximum_capacity / page_size as u64) as usize + initial_capacity;
        let buffer_pool = DynamicPool::new(
            initial_capacity,
            maximum_capacity,
            StreamCacheBuffer::default,
        );

        let fw = Builder::new()
            .append(true)
            .tempfile()
            .map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
        //NamedTempFile::new().map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
        let fr = fw
            .reopen()
            .map_err(|e| anyhow!("err:fw.reopen => e:{}", e))?;
        let fw = Arc::new(Mutex::new(fw));

        #[cfg(unix)]
        let fr_fd = fr.as_raw_fd();

        let fr = Arc::new(Mutex::new(fr));

        Ok(StreamStreamCacheOrFile {
            left_buffer: None,
            read_buffer: None,
            read_err: None,
            stream_status: StreamStatus::DataEmpty,
            caches: VecDeque::new(),
            buffer_pool,
            stream_info,
            r,
            w,
            #[cfg(unix)]
            sendfile,
            is_client,
            fw_seek: 0,
            fw,
            #[cfg(unix)]
            fr_fd,
            fr,
            write_err: None,
        })
    }

    pub fn is_read_empty(&self) -> bool {
        if self.read_buffer.is_none()
            || (self.read_buffer.is_some() && self.read_buffer.as_ref().unwrap().read_size == 0)
        {
            return true;
        }
        return false;
    }

    pub fn is_write_empty(&self) -> bool {
        if self.caches.len() <= 0 && self.left_buffer.is_none() {
            return true;
        }
        return false;
    }
    pub async fn start(
        &mut self,
        limit: StreamLimit,
        stream_config_context: Rc<StreamConfigContext>,
        _thread_id: std::thread::ThreadId,
        _tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 && stream_config_context.stream.stream_cache_size > 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("file_and_cache".to_string());
        } else if limit.tmp_file_size > 0 && stream_config_context.stream.stream_cache_size <= 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("file".to_string());
        } else if limit.tmp_file_size <= 0 && stream_config_context.stream.stream_cache_size > 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("cache".to_string());
        }

        self.stream_info
            .borrow_mut()
            .add_work_time(&format!("{} file", StreamStream::get_flag(self.is_client)));

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = StreamStreamContext {
            stream_cache_size: stream_config_context.stream.stream_cache_size as i64,
            tmp_file_size: limit.tmp_file_size as i64,
            limit_rate_after: limit.limit_rate_after as i64,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: stream_config_context.stream.stream_cache_size as u64,
            max_tmp_file_size: limit.tmp_file_size,
            is_tmp_file_io_page: stream_config_context.stream.is_tmp_file_io_page,
            page_size,
            min_merge_cache_buffer_size: (page_size / 2) as u64,
            min_cache_buffer_size: page_size * *default_config::MIN_CACHE_BUFFER_NUM,
            min_read_buffer_size: page_size,
            min_cache_file_size: page_size * 256,
            total_read_size: 0,
            total_write_size: 0,
        };

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size < ssc.min_cache_buffer_size as i64 {
            ssc.stream_cache_size = ssc.min_cache_buffer_size as i64;
            ssc.max_stream_cache_size = ssc.min_cache_buffer_size as u64;
        }

        if ssc.tmp_file_size > 0 && ssc.tmp_file_size < ssc.min_cache_file_size as i64 {
            ssc.tmp_file_size = ssc.min_cache_file_size as i64;
            ssc.max_tmp_file_size = ssc.min_cache_file_size as u64;
        }

        if ssc.stream_cache_size > 0 {
            let stream_cache_size =
                (ssc.stream_cache_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.stream_cache_size = stream_cache_size;
            ssc.max_stream_cache_size = stream_cache_size as u64;
        }

        if ssc.tmp_file_size > 0 {
            let tmp_file_size =
                (ssc.tmp_file_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.tmp_file_size = tmp_file_size;
            ssc.max_tmp_file_size = tmp_file_size as u64;
        }

        let mut is_first = true;
        let mut _is_runing = true;
        loop {
            if self.stream_info.borrow().debug_is_open_print {
                let stream_info = self.stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:read file stream_cache_size:{}, tmp_file_size:{}, \
                    read_err.is_none:{}, write_err.is_none:{}, is_read_empty:{}, \
                    is_write_empty:{}, is_sendfile_close:{}, max_stream_cache_size:{}, max_tmp_file_size:{}, \
                    total_read_size:{}, total_write_size:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(self.is_client),
                    ssc.stream_cache_size,
                    ssc.tmp_file_size,
                    self.read_err.is_none(),
                    self.write_err.is_none(),
                    self.is_read_empty(),
                    self.is_write_empty(),
                    self.is_sendfile_close(),
                    ssc.max_stream_cache_size,
                    ssc.max_tmp_file_size,
                    ssc.total_read_size,
                    ssc.total_write_size,
                );
            }

            if _is_runing
                && (self.write_err.is_some()
                    || (self.read_err.is_some() && self.is_read_empty() && self.is_write_empty())
                    || (!self.r.is_single().await
                        && self.stream_info.borrow().close_num >= 1
                        && self.is_read_empty()
                        && self.is_write_empty()
                        && !is_first)
                    || self.is_sendfile_close())
            {
                _is_runing = false;
                self.stream_info.borrow_mut().close_num += 1;
                loop {
                    if self.stream_info.borrow().close_num >= 2 {
                        #[cfg(feature = "anydebug")]
                        {
                            let stream_info = self.stream_info.borrow();
                            log::info!(
                                    "{}---{:?}---{}:read close file stream_cache_size:{}, tmp_file_size:{}, \
                                read_err.is_none:{}, write_err.is_none:{}, is_read_empty:{}, \
                                is_write_empty:{}, is_sendfile_close:{}, max_stream_cache_size:{}, max_tmp_file_size:{}, \
                                total_read_size:{}, total_write_size:{}",
                                    stream_info.request_id,
                                    stream_info.server_stream_info.local_addr,
                                    StreamStream::get_flag(self.is_client),
                                    ssc.stream_cache_size,
                                    ssc.tmp_file_size,
                                    self.read_err.is_none(),
                                    self.write_err.is_none(),
                                    self.is_read_empty(),
                                    self.is_write_empty(),
                                    self.is_sendfile_close(),
                                    ssc.max_stream_cache_size,
                                    ssc.max_tmp_file_size,
                                    ssc.total_read_size,
                                    ssc.total_write_size,
                                );
                        }

                        if self.write_err.is_some() {
                            self.write_err.take().unwrap()?;
                        }

                        if self.read_err.is_some() {
                            self.read_err.take().unwrap()?;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        NORMAL_SLEEP_TIME_MILLIS + 100,
                    ))
                    .await;
                }
            }

            if (ssc.stream_cache_size > 0 || ssc.tmp_file_size > 0) && self.read_err.is_none() {
                let (ret, buffer) = self
                    .read(&mut ssc)
                    .await
                    .map_err(|e| anyhow!("err:read => e:{}", e))?;
                if is_first {
                    is_first = false;
                    self.stream_info
                        .borrow_mut()
                        .add_work_time(StreamStream::get_flag(self.is_client));
                }
                self.write_buffer(ret, buffer, &mut ssc)
                    .await
                    .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
            } else {
                self.check_sleep().await;
                if self.read_err.is_some() && self.read_buffer.is_some() {
                    let buffer = self.read_buffer.take().unwrap();
                    if buffer.read_size <= 0 {
                        log::error!("err:buffer.read_size <= 0");
                    } else {
                        self.write_buffer(Ok(0), buffer, &mut ssc)
                            .await
                            .map_err(|e| anyhow!("err:write_buffer => e:{}", e))?;
                    }
                }
            }

            // #[cfg(feature = "anydebug")]
            // {
            //     let stream_info = self.stream_info.borrow();
            //     log::info!(
            //             "{}---{:?}---{}:read file stream_cache_size:{}, tmp_file_size:{}, \
            //                     read_err.is_none:{}, write_err.is_none:{}, is_read_empty:{}, \
            //                     is_write_empty:{}, is_sendfile_close:{}, max_stream_cache_size:{}, max_tmp_file_size:{}, \
            //                     total_read_size:{}, total_write_size:{}",
            //             stream_info.request_id,
            //             stream_info.server_stream_info.local_addr,
            //             StreamStream::get_flag(self.is_client),
            //             ssc.stream_cache_size,
            //             ssc.tmp_file_size,
            //             self.read_err.is_none(),
            //             self.write_err.is_none(),
            //             self.is_read_empty(),
            //             self.is_write_empty(),
            //             self.is_sendfile_close(),
            //             ssc.max_stream_cache_size,
            //             ssc.max_tmp_file_size,
            //             ssc.total_read_size,
            //             ssc.total_write_size,
            //         );
            // }

            self.stream_status = self
                .write(&mut ssc)
                .await
                .map_err(|e| anyhow!("err:write => e:{}", e))?;
        }
    }

    pub fn is_sendfile_close(&self) -> bool {
        #[cfg(unix)]
        {
            let is_stream_full = if let StreamStatus::StreamFull(_) = self.stream_status {
                true
            } else {
                false
            };
            if is_stream_full && self.stream_info.borrow().close_num >= 1 && self.sendfile.is_some()
            {
                return true;
            }
        }
        return false;
    }

    pub async fn read(
        &mut self,
        ssc: &mut StreamStreamContext,
    ) -> Result<(std::io::Result<usize>, DynamicPoolItem<StreamCacheBuffer>)> {
        let mut buffer = if self.read_buffer.is_some() {
            self.read_buffer.take().unwrap()
        } else {
            self.buffer_pool.take()
        };

        buffer.resize(None);
        buffer.is_cache = false;

        if buffer.size < buffer.read_size {
            log::error!("err:buffer.size < buffer.read_size");
            return Err(anyhow!("err:buffer.size < buffer.read_size"));
        }

        if ssc.stream_cache_size < buffer.read_size as i64
            && ssc.tmp_file_size < buffer.read_size as i64
        {
            log::error!("err:buffer.read_size");
            return Err(anyhow!("err:buffer.read_size"));
        }

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size >= buffer.read_size as i64 {
            buffer.is_cache = true;
            if ssc.stream_cache_size < buffer.size as i64 {
                buffer.size = ssc.stream_cache_size as u64;
            }
        }

        if !buffer.is_cache {
            if ssc.tmp_file_size < buffer.size as i64 {
                buffer.size = ssc.tmp_file_size as u64;
            }
        }

        let ret: std::io::Result<usize> = async {
            if buffer.size == buffer.read_size {
                if self.is_write_empty() {
                    return Ok(0);
                }
                self.check_sleep().await;
                return Ok(0);
            }

            let sleep_read_time_millis = match &self.stream_status {
                StreamStatus::Limit => LIMIT_SLEEP_TIME_MILLIS,
                StreamStatus::StreamFull(info) => {
                    if info.is_sendfile {
                        SENDFILE_FULL_SLEEP_TIME_MILLIS
                    } else {
                        NOT_SLEEP_TIME_MILLIS
                    }
                }
                StreamStatus::Ok(_) => NOT_SLEEP_TIME_MILLIS,
                StreamStatus::DataEmpty => {
                    if buffer.read_size > 0 {
                        MIN_SLEEP_TIME_MILLIS
                    } else {
                        NORMAL_SLEEP_TIME_MILLIS
                    }
                }
            };

            let read_size = buffer.read_size as usize;
            let end_size = buffer.size as usize;
            if self.r.is_read_msg().await {
                tokio::select! {
                    biased;
                    ret = self.r.read_msg() => {
                        let msg = ret?;
                        let n = msg.len();
                        buffer.msg = Some(msg);
                        return Ok(n);
                    },
                    _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                        return Ok(0);
                    },
                    else => {
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                        "err:do_stream_to_stream_or_file select close"
                    )));
                    }
                }
            } else {
                tokio::select! {
                    biased;
                    ret = self.r.read(buffer.data(read_size, end_size)) => {
                        let n = ret?;
                        return Ok(n);
                    },
                    _= tokio::time::sleep(std::time::Duration::from_millis(sleep_read_time_millis)) => {
                        return Ok(0);
                    },
                    else => {
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, anyhow!(
                        "err:do_stream_to_stream_or_file select close"
                    )));
                    }
                }
            }
        }
        .await;

        Ok((ret, buffer))
    }

    pub async fn check_sleep(&self) {
        match &self.stream_status {
            StreamStatus::Limit => {
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            StreamStatus::StreamFull(info) => {
                let sleep_time = if info.is_sendfile {
                    SENDFILE_FULL_SLEEP_TIME_MILLIS
                } else {
                    LIMIT_SLEEP_TIME_MILLIS
                };
                tokio::time::sleep(std::time::Duration::from_millis(sleep_time)).await;
            }
            StreamStatus::Ok(_) => {
                log::error!("err:StreamStatus::Ok");
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
            StreamStatus::DataEmpty => {
                log::error!("err:StreamStatus::DataEmpty");
                tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS)).await;
            }
        };
    }

    pub async fn write_buffer(
        &mut self,
        ret: std::io::Result<usize>,
        mut buffer: DynamicPoolItem<StreamCacheBuffer>,
        ssc: &mut StreamStreamContext,
    ) -> Result<()> {
        if let Err(ref e) = ret {
            log::trace!(
                "{}, read_err = Some(ret)",
                StreamStream::get_flag(self.is_client)
            );
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                return Err(anyhow!("err:{}", e))?;
            }
            self.read_err = Some(Err(anyhow!(
                "err:stream read => flag:{}, e:{}",
                StreamStream::get_flag(self.is_client),
                e
            )));
            if buffer.read_size == 0 {
                return Ok(());
            }
        } else {
            let size = ret? as u64;
            ssc.total_read_size += size;
            self.stream_info.borrow_mut().total_read_size += size;
            let old_read_size = buffer.read_size;
            buffer.read_size += size;

            log::trace!(
                "{}, read size:{}, buffer.read_size:{}",
                StreamStream::get_flag(self.is_client),
                size,
                buffer.read_size
            );

            if buffer.msg.is_none() && buffer.size != buffer.read_size {
                if !self.is_write_empty() {
                    if buffer.read_size < ssc.min_read_buffer_size as u64 {
                        self.read_buffer = Some(buffer);
                        return Ok(());
                    }
                } else {
                    if old_read_size <= 0 && buffer.read_size < ssc.min_read_buffer_size as u64 {
                        self.read_buffer = Some(buffer);
                        return Ok(());
                    }
                }
            }
        }

        buffer.size = buffer.read_size;
        let mut size = buffer.size;
        if self.stream_info.borrow().debug_is_open_print {
            let stream_info = self.stream_info.borrow();
            log::info!(
                "{}---{:?}---{}:read file is_cache:{}, size:{}",
                stream_info.request_id,
                stream_info.server_stream_info.local_addr,
                StreamStream::get_flag(self.is_client),
                buffer.is_cache,
                size,
            );
        }

        if buffer.is_cache {
            ssc.stream_cache_size -= size as i64;
            let last_cache = self.caches.back_mut();
            if buffer.msg.is_none() && last_cache.is_some() {
                let last_cache = last_cache.unwrap();
                if let StreamCache::Buffer(last_cache) = last_cache {
                    if last_cache.msg.is_none() {
                        if last_cache.size < ssc.min_merge_cache_buffer_size
                            || buffer.size < ssc.min_merge_cache_buffer_size
                        {
                            let buffer_end = buffer.size as usize;
                            let resize = last_cache.size as usize;
                            last_cache.resize(Some(resize));
                            last_cache
                                .data_raw()
                                .extend_from_slice(buffer.data(0, buffer_end));
                            last_cache.size += buffer.size;
                            return Ok(());
                        }
                    }
                }
            }
            self.caches.push_back(StreamCache::Buffer(buffer));
            return Ok(());
        }

        if buffer.msg.is_none() && ssc.is_tmp_file_io_page {
            let next_seek_end = (self.fw_seek / ssc.page_size as u64 + 1) * ssc.page_size as u64;
            let fw_seek_end = self.fw_seek + size;
            let fw_seek_page = (fw_seek_end / ssc.page_size as u64) * ssc.page_size as u64;
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
                    let mut left_buffer = self.buffer_pool.take();
                    left_buffer
                        .data_raw()
                        .extend_from_slice(buffer.data(copy_start as usize, size as usize));
                    left_buffer.read_size = left_size;
                    left_buffer.resize(None);
                    self.read_buffer = Some(left_buffer);

                    size = buffer.size;
                }
            }
        }

        ssc.tmp_file_size -= size as i64;
        let fw = self.fw.clone();
        let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
            let mut fw = fw.lock().unwrap();
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

        if ssc.is_tmp_file_io_page {
            let mut is_first = true;
            let mut size = size;
            loop {
                if size <= 0 {
                    break;
                }

                let w_size = if is_first {
                    ssc.page_size as u64 - self.fw_seek % ssc.page_size as u64
                } else {
                    ssc.page_size as u64
                };
                let w_size = if w_size > size { size } else { w_size };

                let seek = self.fw_seek;
                self.fw_seek += w_size;
                size -= w_size;

                if is_first {
                    is_first = false;
                    if w_size != ssc.page_size as u64 {
                        let last_cache = self.caches.back_mut();
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
                self.caches
                    .push_back(StreamCache::File(StreamCacheFile { seek, size: w_size }));
            }

            return Ok(());
        }

        let seek = self.fw_seek;
        self.fw_seek += size;

        let last_cache = self.caches.back_mut();
        if last_cache.is_some() {
            let last_cache = last_cache.unwrap();
            if let StreamCache::File(last_cache) = last_cache {
                if last_cache.size < ssc.min_merge_cache_buffer_size
                    || size < ssc.min_merge_cache_buffer_size
                {
                    if last_cache.seek + last_cache.size != seek {
                        log::error!("err:last_cache.seek + last_cache.size != seek")
                    }
                    last_cache.size += size;
                    return Ok(());
                }
            }
        }

        self.caches
            .push_back(StreamCache::File(StreamCacheFile { seek, size }));

        return Ok(());
    }

    pub async fn read_buffer(&mut self) -> Result<Option<DynamicPoolItem<StreamCacheBuffer>>> {
        if self.left_buffer.is_some() {
            log::trace!("{}, left_buffer", StreamStream::get_flag(self.is_client));
            Ok(self.left_buffer.take())
        } else {
            log::trace!("{}, caches", StreamStream::get_flag(self.is_client));
            let cache = self.caches.pop_front();
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
                    let mut buffer = self.buffer_pool.take();
                    buffer.resize(Some(file.size as usize));
                    buffer.size = file.size;
                    buffer.seek = file.seek;

                    let ret = loop {
                        #[cfg(unix)]
                        if self.sendfile.is_some() {
                            buffer.file_fd = self.fr_fd;
                            break Ok(buffer);
                        }

                        let fr = self.fr.clone();
                        let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> =
                            tokio::task::spawn_blocking(move || {
                                let end_size = buffer.size as usize;
                                let ret = fr
                                    .lock()
                                    .unwrap()
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

    pub async fn write(&mut self, ssc: &mut StreamStreamContext) -> Result<StreamStatus> {
        self.stream_info.borrow_mut().is_break_stream_write = false;
        loop {
            let buffer = self.read_buffer().await?;
            if buffer.is_none() {
                return Ok(StreamStatus::DataEmpty);
            }
            let mut buffer = buffer.unwrap();
            log::trace!(
                "{}, start:{}, size:{}",
                StreamStream::get_flag(self.is_client),
                buffer.start,
                buffer.size
            );

            if self.stream_info.borrow().debug_is_open_print {
                let stream_info = self.stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:start stream_limit_write buffer.start:{}, buffer.size:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(self.is_client),
                    buffer.start,
                    buffer.size,
                );
            }

            let stream_status = StreamStream::stream_limit_write(
                &mut buffer,
                ssc,
                &mut self.w,
                self.is_client,
                #[cfg(unix)]
                &self.sendfile,
                self.stream_info.clone(),
            )
            .await;

            if let Err(ref e) = stream_status {
                log::debug!(
                    "{}, write_err = Some(ret)",
                    StreamStream::get_flag(self.is_client)
                );
                if e.kind() != std::io::ErrorKind::ConnectionReset {
                    return Err(anyhow!("err:StreamStream.stream_write => e:{}", e))?;
                }
                self.write_err = Some(Err(anyhow!(
                    "err:stream_write => flag:{}, e:{}",
                    StreamStream::get_flag(self.is_client),
                    e
                )));
                return Ok(StreamStatus::StreamFull(StreamFullInfo::default()));
            }
            let stream_status = stream_status.unwrap();

            if self.stream_info.borrow().debug_is_open_print {
                let stream_info = self.stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:end stream_limit_write stream_status:{:?}, stream_cache_size:{}, max_tmp_file_size:{}, tmp_file_size:{} ",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(self.is_client),
                    stream_status,
                    ssc.stream_cache_size,
                    ssc.max_tmp_file_size,
                    ssc.tmp_file_size,
                );
            }

            if buffer.start < buffer.size {
                self.left_buffer = Some(buffer);
            }
            match &stream_status {
                StreamStatus::Limit => {
                    return Ok(stream_status);
                }
                StreamStatus::StreamFull(_) => {
                    return Ok(stream_status);
                }
                StreamStatus::Ok(_) => {
                    //
                }
                StreamStatus::DataEmpty => {
                    log::error!("err:StreamStatus::DataEmpty");
                }
            }

            if self.stream_info.borrow().is_break_stream_write {
                return Ok(StreamStatus::StreamFull(StreamFullInfo::default()));
            }
        }
    }
}
