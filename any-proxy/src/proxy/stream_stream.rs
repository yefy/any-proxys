#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_var;
use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use super::StreamConfigContext;
use super::StreamLimit;
use super::StreamStreamContext;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::protopack;
use crate::stream::stream_flow;
use crate::util::default_config;
use anyhow::Result;
use anyhow::{anyhow, Error};
use dynamic_pool::{DynamicPool, DynamicPoolItem};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tempfile::NamedTempFile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const LIMIT_SLEEP_TIME_MILLIS: u64 = 100;
const NORMAL_SLEEP_TIME_MILLIS: u64 = 1000 * 1;
const NOT_SLEEP_TIME_MILLIS: u64 = 0;
const MIN_SLEEP_TIME_MILLIS: u64 = 10;
const SENDFILE_FULL_SLEEP_TIME_MILLIS: u64 = 30;

#[derive(Debug)]
pub struct StreamFullInfo {
    is_sendfile: bool,
    _write_size: u64,
}

impl Default for StreamFullInfo {
    fn default() -> Self {
        StreamFullInfo::new(false, 0)
    }
}

impl StreamFullInfo {
    pub fn new(is_sendfile: bool, _write_size: u64) -> StreamFullInfo {
        StreamFullInfo {
            is_sendfile,
            _write_size,
        }
    }
}

#[derive(Debug)]
pub enum StreamStatus {
    Limit,
    StreamFull(StreamFullInfo),
    Ok(u64),
    DataEmpty,
}

pub struct StreamStream {}

impl StreamStream {
    pub fn get_flag(is_client: bool) -> &'static str {
        if is_client {
            "client -> upstream"
        } else {
            "upstream -> client"
        }
    }

    pub async fn connect_and_stream(
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_buffer: &[u8],
        #[cfg(feature = "anyproxy-ebpf")] mut client_stream: stream_flow::StreamFlow,
        #[cfg(not(feature = "anyproxy-ebpf"))] client_stream: stream_flow::StreamFlow,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
        client_local_addr: SocketAddr,
        client_remote_addr: SocketAddr,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    ) -> Result<()> {
        stream_info.borrow_mut().add_work_time("connect");

        let (is_proxy_protocol_hello, connect_info) = {
            let client_ip = stream_var::client_ip(&stream_info.borrow());
            if client_ip.is_none() {
                return Err(anyhow!("err: client_ip nil"));
            }
            let client_ip = client_ip.unwrap();

            stream_info.borrow_mut().err_status = ErrStatus::ServiceUnavailable;
            let (ups_dispatch, connect_info) =
                { config_ctx.ups_data.lock().unwrap().get_connect(&client_ip) };
            stream_info.borrow_mut().ups_dispatch = Some(ups_dispatch);
            if connect_info.is_none() {
                return Err(anyhow!("err: connect_func nil"));
            }
            let upstream_connect_flow_info =
                stream_info.borrow().upstream_connect_flow_info.clone();
            let (is_proxy_protocol_hello, connect_func) = connect_info.unwrap();
            let connect_info = connect_func
                .connect(&mut Some(&mut upstream_connect_flow_info.borrow_mut()))
                .await
                .map_err(|e| anyhow!("err:connect => e:{}", e))?;
            (is_proxy_protocol_hello, connect_info)
        };

        let (mut upstream_stream, upstream_connect_info) = connect_info;

        #[cfg(feature = "anyproxy-ebpf")]
        let ups_remote_addr = upstream_connect_info.remote_addr.clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let ups_local_addr = upstream_connect_info.local_addr.clone();

        log::debug!(
            "skip ebpf warning ups_local_addr:{}, ups_remote_addr:{}",
            upstream_connect_info.local_addr,
            upstream_connect_info.remote_addr
        );

        log::debug!(
            "upstream_protocol_name:{}",
            upstream_connect_info.protocol7.to_string()
        );

        let is_proxy_protocol_hello = {
            let mut stream_info = stream_info.borrow_mut();
            upstream_stream.set_stream_info(Some(stream_info.upstream_stream_flow_info.clone()));
            stream_info.upstream_connect_info = Some(upstream_connect_info);
            stream_info.err_status = ErrStatus::Ok;

            let is_proxy_protocol_hello = if is_proxy_protocol_hello.is_none() {
                if config_ctx.is_proxy_protocol_hello.is_none() {
                    false
                } else {
                    config_ctx.is_proxy_protocol_hello.unwrap()
                }
            } else {
                is_proxy_protocol_hello.unwrap()
            };
            stream_info.is_proxy_protocol_hello = is_proxy_protocol_hello;
            is_proxy_protocol_hello
        };

        if is_proxy_protocol_hello {
            let protocol_hello = stream_info.borrow().protocol_hello.clone();
            protopack::anyproxy::write_pack(
                &mut upstream_stream,
                protopack::anyproxy::AnyproxyHeaderType::Hello,
                protocol_hello.lock().unwrap().as_ref().unwrap(),
            )
            .await
            .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
        }

        if client_buffer.len() > 0 {
            upstream_stream
                .write(client_buffer)
                .await
                .map_err(|e| anyhow!("err:client_buf_reader.buffer => e:{}", e))?;
        }
        log::debug!(
            "skip ebpf warning client_remote_addr.fd():{},  client_local_addr.fd():{}",
            client_remote_addr,
            client_local_addr
        );

        let mut _is_open_ebpf = false;
        #[cfg(feature = "anyproxy-ebpf")]
        if config_ctx.fast_conf.is_open_ebpf && client_stream.fd() > 0 && upstream_stream.fd() > 0 {
            _is_open_ebpf = true;
            stream_info.borrow_mut().is_discard_timeout = true;
            stream_info.borrow_mut().add_work_time("ebpf");

            stream_info.borrow_mut().is_open_ebpf = true;

            log::info!(
                "ebpf client_stream.fd():{},  upstream_stream.fd():{}",
                client_stream.fd(),
                upstream_stream.fd()
            );
            log::info!(
                "ebpf client_remote_addr.fd():{},  client_local_addr.fd():{}",
                client_remote_addr,
                client_local_addr
            );
            log::info!(
                "ebpf ups_remote_addr.fd():{},  ups_local_addr.fd():{}",
                ups_remote_addr,
                ups_local_addr
            );

            let rx = ebpf_add_sock_hash
                .add_socket_data(
                    client_stream.fd(),
                    client_remote_addr.clone(),
                    client_local_addr.clone(),
                    upstream_stream.fd(),
                    ups_remote_addr,
                    ups_local_addr,
                )
                .await?;
            let _ = rx.recv().await;

            upstream_stream
                .write("".as_bytes())
                .await
                .map_err(|e| anyhow!("err:upstream_stream.write => e:{}", e))?;
            client_stream
                .write("".as_bytes())
                .await
                .map_err(|e| anyhow!("err:client_stream.write => e:{}", e))?;
        }

        stream_info.borrow_mut().add_work_time("stream_to_stream");

        let _client_stream_fd = client_stream.fd();
        let _upstream_stream_fd = upstream_stream.fd();

        #[cfg(unix)]
        let (client_sendfile, upstream_sendfile) =
            if config_ctx.fast_conf.is_open_sendfile && !_is_open_ebpf {
                let client_sendfile = Some(SendFile::new(
                    _client_stream_fd,
                    Some(stream_info.borrow().client_stream_flow_info.clone()),
                ));
                let upstream_sendfile = Some(SendFile::new(
                    _upstream_stream_fd,
                    Some(stream_info.borrow().upstream_stream_flow_info.clone()),
                ));
                (client_sendfile, upstream_sendfile)
            } else {
                (None, None)
            };

        // log::info!("stream_to_stream");
        let ret = StreamStream::stream_to_stream(
            config_ctx.clone(),
            stream_info.clone(),
            client_stream,
            upstream_stream,
            #[cfg(unix)]
            client_sendfile,
            #[cfg(unix)]
            upstream_sendfile,
            thread_id,
            tmp_file_id,
        )
        .await;

        #[cfg(feature = "anyproxy-ebpf")]
        if config_ctx.fast_conf.is_open_ebpf && _client_stream_fd > 0 && _upstream_stream_fd > 0 {
            let rx = ebpf_add_sock_hash
                .del_socket_data(
                    _client_stream_fd,
                    client_remote_addr.clone(),
                    client_local_addr.clone(),
                    _upstream_stream_fd,
                    ups_remote_addr,
                    ups_local_addr,
                )
                .await?;
            let _ = rx.recv().await;
        }

        stream_info.borrow_mut().add_work_time("end");

        ret
    }

    pub async fn stream_to_stream(
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        client: stream_flow::StreamFlow,
        upstream: stream_flow::StreamFlow,
        #[cfg(unix)] client_sendfile: Option<SendFile>,
        #[cfg(unix)] upstream_sendfile: Option<SendFile>,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        let (client_read, client_write) = client.split();
        let (upstream_read, upstream_write) = upstream.split();
        let download_limit = StreamLimit::new(
            config_ctx.tmp_file.download_tmp_file_size,
            config_ctx.rate.download_limit_rate_after,
            config_ctx.rate.download_limit_rate,
        );
        let upload_limit = StreamLimit::new(
            config_ctx.tmp_file.upload_tmp_file_size,
            config_ctx.rate.upload_limit_rate_after,
            config_ctx.rate.upload_limit_rate,
        );

        let ret: Result<()> = async {
            tokio::select! {
                _ = StreamStream::limit_timeout(upload_limit.clone(), download_limit.clone()) => {
                    Ok(())
                }
                ret = StreamStream::do_stream_to_stream(
                    upload_limit,
                    config_ctx.clone(),
                    stream_info.clone(),
                    client_read,
                    upstream_write,
                    #[cfg(unix)] upstream_sendfile,
                    true,
                    thread_id.clone(),
                    tmp_file_id.clone(),
                )  => {
                    return ret;
                }
                ret =  StreamStream::do_stream_to_stream(
                    download_limit,
                    config_ctx,
                    stream_info,
                    upstream_read,
                    client_write,
                    #[cfg(unix)] client_sendfile,
                    false,
                    thread_id,
                    tmp_file_id,
                ) => {
                    return ret;
                }
                else => {
                    return Err(anyhow!("err:stream_to_stream_or_file select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn limit_timeout(upload_limit: StreamLimit, download_limit: StreamLimit) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            upload_limit
                .curr_limit_rate
                .store(upload_limit.max_limit_rate, Ordering::Relaxed);
            download_limit
                .curr_limit_rate
                .store(download_limit.max_limit_rate, Ordering::Relaxed);
        }
    }

    pub async fn do_stream_to_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        r: R,
        w: W,
        #[cfg(unix)] sendfile: Option<SendFile>,
        is_client: bool,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 || config_ctx.stream.stream_cache_size > 0 {
            let mut cache_or_file = StreamStreamCacheOrFile::new(
                stream_info,
                r,
                w,
                #[cfg(unix)]
                sendfile,
                is_client,
                config_ctx.stream.stream_cache_size,
            )?;
            cache_or_file
                .start(limit, config_ctx, thread_id, tmp_file_id)
                .await
        } else {
            let mut memory = StreamStreamMemory::new();
            memory.start(limit, stream_info, r, w, is_client).await
        }
    }

    pub async fn stream_limit_write<W: AsyncWrite + Unpin>(
        buffer: &mut StreamCacheBuffer,
        ssc: &mut StreamStreamContext,
        w: &mut W,
        is_client: bool,
        #[cfg(unix)] sendfile: &Option<SendFile>,
    ) -> std::io::Result<(StreamStatus, u128)> {
        let mut _write_max_block_time_ms = 0;
        let mut _is_sendfile = false;
        let mut n = buffer.size - buffer.start;
        let curr_limit_rate = ssc.curr_limit_rate.load(Ordering::Relaxed);
        let limit_rate_size = if ssc.max_limit_rate <= 0 {
            n
        } else if ssc.limit_rate_after > 0 {
            ssc.limit_rate_after
        } else if curr_limit_rate > 0 {
            curr_limit_rate
        } else {
            return Ok((StreamStatus::Limit, _write_max_block_time_ms));
        };

        if limit_rate_size < n {
            n = limit_rate_size;
        }
        log::trace!("{}, n:{}", StreamStream::get_flag(is_client), n);

        let wn = loop {
            #[cfg(unix)]
            if sendfile.is_some() && buffer.file_fd > 0 {
                _is_sendfile = true;
                let wn = sendfile
                    .as_ref()
                    .unwrap()
                    .write(buffer.file_fd, buffer.seek, n)
                    .await;
                if let Err(e) = wn {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        return Ok((
                            StreamStatus::StreamFull(StreamFullInfo::new(_is_sendfile, 0)),
                            _write_max_block_time_ms,
                        ));
                    }
                    return Err(e);
                }
                break wn.unwrap();
            }

            _is_sendfile = false;
            let end = buffer.start + n;
            if false {
                let ret = tokio::time::timeout(
                    tokio::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS),
                    w.write(&buffer.datas[buffer.start as usize..end as usize]),
                )
                .await;
                if ret.is_err() {
                    return Ok((
                        StreamStatus::StreamFull(StreamFullInfo::default()),
                        _write_max_block_time_ms,
                    ));
                }
                break ret.unwrap()? as u64;
            } else {
                #[cfg(feature = "anyproxy-write-block-time-ms")]
                let write_start_time = Instant::now();
                let write_size = w
                    .write(&buffer.datas[buffer.start as usize..end as usize])
                    .await? as u64;
                #[cfg(feature = "anyproxy-write-block-time-ms")]
                {
                    _write_max_block_time_ms = write_start_time.elapsed().as_millis();
                }
                break write_size;
            };
        };

        log::trace!("{}, wn:{}", StreamStream::get_flag(is_client), wn);
        if ssc.max_limit_rate <= 0 {
            //
        } else if ssc.limit_rate_after > 0 {
            ssc.limit_rate_after -= wn;
        } else {
            ssc.curr_limit_rate.fetch_sub(wn, Ordering::Relaxed);
        }
        buffer.start += wn;
        buffer.seek += wn;
        if buffer.is_cache {
            if ssc.max_stream_cache_size > 0 {
                ssc.stream_cache_size += wn;
            }
        } else {
            if ssc.max_tmp_file_size > 0 {
                ssc.tmp_file_size += wn;
            }
        }

        if wn != n {
            return Ok((
                StreamStatus::StreamFull(StreamFullInfo::new(_is_sendfile, wn)),
                _write_max_block_time_ms,
            ));
        }
        return Ok((StreamStatus::Ok(wn), _write_max_block_time_ms));
    }
}

pub struct StreamStreamMemory {}

impl StreamStreamMemory {
    pub fn new() -> StreamStreamMemory {
        StreamStreamMemory {}
    }
    pub async fn start<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &mut self,
        limit: StreamLimit,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: R,
        mut w: W,
        is_client: bool,
    ) -> Result<()> {
        stream_info.borrow_mut().buffer_cache = Some("memory".to_string());
        stream_info
            .borrow_mut()
            .add_work_time(&format!("{} cache", StreamStream::get_flag(is_client)));

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = StreamStreamContext {
            stream_cache_size: 0,
            tmp_file_size: 0,
            limit_rate_after: limit.limit_rate_after,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: 0,
            max_tmp_file_size: 0,
            is_tmp_file_io_page: true,
            page_size,
            min_merge_cache_buffer_size: (page_size / 2) as u64,
            min_cache_buffer_size: page_size * *default_config::MIN_CACHE_BUFFER_NUM,
            min_read_buffer_size: page_size,
            min_cache_file_size: page_size * 256,
        };

        let mut is_first = true;
        let mut buffer = StreamCacheBuffer::new();
        buffer.is_cache = true;
        loop {
            buffer.reset();
            buffer.resize(None);
            buffer.is_cache = true;
            buffer.size = r
                .read(&mut buffer.datas)
                .await
                .map_err(|e| anyhow!("err:r.read => e:{}", e))? as u64;

            if stream_info.borrow().debug_is_open_print {
                let stream_info = stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:read buffer.size:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(is_client),
                    buffer.size,
                );
            }

            if is_first {
                is_first = false;
                stream_info
                    .borrow_mut()
                    .add_work_time(StreamStream::get_flag(is_client));
            }

            loop {
                if buffer.start >= buffer.size {
                    break;
                }

                if stream_info.borrow().debug_is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{:?}---{}:write start:{}, size:{}",
                        stream_info.request_id,
                        stream_info.server_stream_info.local_addr,
                        StreamStream::get_flag(is_client),
                        buffer.start,
                        buffer.size
                    );
                }

                let (stream_status, _write_max_block_time_ms) = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut ssc,
                    &mut w,
                    is_client,
                    #[cfg(unix)]
                    &None,
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;
                if stream_info.borrow().debug_is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{:?}---{}: stream_status:{:?}",
                        stream_info.request_id,
                        stream_info.server_stream_info.local_addr,
                        StreamStream::get_flag(is_client),
                        stream_status
                    );
                }

                #[cfg(feature = "anyproxy-write-block-time-ms")]
                {
                    if _write_max_block_time_ms > stream_info.borrow().write_max_block_time_ms {
                        stream_info.borrow_mut().write_max_block_time_ms = _write_max_block_time_ms;
                    }
                }

                if let StreamStatus::Limit = stream_status {
                    tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS))
                        .await;
                }
            }
        }
    }
}

pub struct StreamStreamCacheOrFile<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> {
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

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> StreamStreamCacheOrFile<R, W> {
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

        let fw = NamedTempFile::new().map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
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
    pub fn is_empty(&self) -> bool {
        if self.caches.len() <= 0
            && self.left_buffer.is_none()
            && (self.read_buffer.is_none()
                || (self.read_buffer.is_some()
                    && self.read_buffer.as_ref().unwrap().read_size == 0))
        {
            return true;
        }
        return false;
    }
    pub async fn start(
        &mut self,
        limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        _thread_id: std::thread::ThreadId,
        _tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 && config_ctx.stream.stream_cache_size > 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("file_and_cache".to_string());
        } else if limit.tmp_file_size > 0 && config_ctx.stream.stream_cache_size <= 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("file".to_string());
        } else if limit.tmp_file_size <= 0 && config_ctx.stream.stream_cache_size > 0 {
            self.stream_info.borrow_mut().buffer_cache = Some("cache".to_string());
        }

        self.stream_info
            .borrow_mut()
            .add_work_time(&format!("{} file", StreamStream::get_flag(self.is_client)));

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = StreamStreamContext {
            stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            tmp_file_size: limit.tmp_file_size,
            limit_rate_after: limit.limit_rate_after,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            max_tmp_file_size: limit.tmp_file_size,
            is_tmp_file_io_page: config_ctx.stream.is_tmp_file_io_page,
            page_size,
            min_merge_cache_buffer_size: (page_size / 2) as u64,
            min_cache_buffer_size: page_size * *default_config::MIN_CACHE_BUFFER_NUM,
            min_read_buffer_size: page_size,
            min_cache_file_size: page_size * 256,
        };

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size < ssc.min_cache_buffer_size as u64 {
            ssc.stream_cache_size = ssc.min_cache_buffer_size as u64;
            ssc.max_stream_cache_size = ssc.min_cache_buffer_size as u64;
        }

        if ssc.tmp_file_size > 0 && ssc.tmp_file_size < ssc.min_cache_file_size as u64 {
            ssc.tmp_file_size = ssc.min_cache_file_size as u64;
            ssc.max_tmp_file_size = ssc.min_cache_file_size as u64;
        }

        if ssc.stream_cache_size > 0 {
            let stream_cache_size =
                (ssc.stream_cache_size / ssc.page_size as u64 + 1) * ssc.page_size as u64;
            ssc.stream_cache_size = stream_cache_size;
            ssc.max_stream_cache_size = stream_cache_size;
        }

        if ssc.tmp_file_size > 0 {
            let tmp_file_size =
                (ssc.tmp_file_size / ssc.page_size as u64 + 1) * ssc.page_size as u64;
            ssc.tmp_file_size = tmp_file_size;
            ssc.max_tmp_file_size = tmp_file_size;
        }

        let mut is_first = true;
        let mut _is_runing = true;
        loop {
            if self.stream_info.borrow().debug_is_open_print {
                let stream_info = self.stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:read file stream_cache_size:{}, tmp_file_size:{}, read_err.is_none:{}, write_err.is_none:{}, is_empty:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(self.is_client),
                    ssc.stream_cache_size,
                    ssc.tmp_file_size,
                    self.read_err.is_none(),
                    self.write_err.is_none(),
                    self.is_empty(),
                );
            }

            if _is_runing
                && (self.write_err.is_some()
                    || (self.read_err.is_some() && self.is_empty())
                    || (self.stream_info.borrow().close_num >= 1 && self.is_empty())
                    || self.is_sendfile_close())
            {
                _is_runing = false;
                self.stream_info.borrow_mut().close_num += 1;
                loop {
                    if self.stream_info.borrow().close_num >= 2 {
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
                let (ret, buffer) = self.read(&mut ssc).await?;
                if is_first {
                    is_first = false;
                    self.stream_info
                        .borrow_mut()
                        .add_work_time(StreamStream::get_flag(self.is_client));
                }
                self.write_buffer(ret, buffer, &mut ssc).await?;
            } else {
                self.check_sleep().await;
            }
            self.stream_status = self.write(&mut ssc).await?;
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

        if ssc.stream_cache_size < buffer.read_size && ssc.tmp_file_size < buffer.read_size {
            log::error!("err:buffer.read_size");
            return Err(anyhow!("err:buffer.read_size"));
        }

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size >= buffer.read_size {
            buffer.is_cache = true;
            if ssc.stream_cache_size < buffer.size {
                buffer.size = ssc.stream_cache_size;
            }
        }

        if !buffer.is_cache {
            if ssc.tmp_file_size < buffer.size {
                buffer.size = ssc.tmp_file_size;
            }
        }

        let ret: std::io::Result<usize> = async {
            if buffer.size == buffer.read_size {
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
            tokio::select! {
                biased;
                ret = self.r.read(&mut buffer.datas[read_size..end_size]) => {
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
            log::debug!(
                "{}, read_err = Some(ret)",
                StreamStream::get_flag(self.is_client)
            );
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                return Err(anyhow!("err:{}", e))?;
            }
            self.read_err = Some(Err(anyhow!("err:{}", e)));
            if buffer.read_size == 0 {
                return Ok(());
            }
        } else {
            let size = ret? as u64;
            let old_read_size = buffer.read_size;
            buffer.read_size += size;
            log::trace!(
                "{}, read size:{}, buffer.read_size:{}",
                StreamStream::get_flag(self.is_client),
                size,
                buffer.read_size
            );

            if buffer.size != buffer.read_size {
                if self.caches.len() > 0 || self.left_buffer.is_some() {
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
            ssc.stream_cache_size -= size;
            let last_cache = self.caches.back_mut();
            if last_cache.is_some() {
                let last_cache = last_cache.unwrap();
                if let StreamCache::Buffer(last_cache) = last_cache {
                    if last_cache.size < ssc.min_merge_cache_buffer_size
                        || buffer.size < ssc.min_merge_cache_buffer_size
                    {
                        let resize = last_cache.size as usize;
                        last_cache.resize(Some(resize));
                        last_cache
                            .datas
                            .extend_from_slice(&buffer.datas[0..buffer.size as usize]);
                        last_cache.size += buffer.size;
                        return Ok(());
                    }
                }
            }
            self.caches.push_back(StreamCache::Buffer(buffer));
            return Ok(());
        }

        if ssc.is_tmp_file_io_page {
            let next_seek_end = (self.fw_seek / ssc.page_size as u64 + 1) * ssc.page_size as u64;
            let fw_seek_end = self.fw_seek + size;
            let fw_seek_page = (fw_seek_end / ssc.page_size as u64) * ssc.page_size as u64;
            if fw_seek_end > next_seek_end {
                let left_size = fw_seek_end - fw_seek_page;
                if left_size > 0 {
                    buffer.size = size - left_size;
                    let copy_start = buffer.size;
                    let mut left_buffer = self.buffer_pool.take();
                    left_buffer
                        .datas
                        .extend_from_slice(&buffer.datas[copy_start as usize..size as usize]);
                    left_buffer.read_size = left_size;
                    left_buffer.size = left_size;
                    self.read_buffer = Some(left_buffer);

                    size = buffer.size;
                }
            }
        }

        ssc.tmp_file_size -= size;
        let fw = self.fw.clone();
        let ret: std::result::Result<(), Error> = tokio::task::spawn_blocking(move || {
            let mut fw = fw.lock().unwrap();
            let ret = fw
                .write_all(&buffer.datas[..size as usize])
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
                                    .read_exact(&mut buffer.datas[..end_size])
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
                    "{}---{:?}---{}:read file buffer.start:{}, buffer.size:{}",
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
                self.write_err = Some(Err(anyhow!("err:StreamStream.stream_write => e:{}", e)));
                return Ok(StreamStatus::StreamFull(StreamFullInfo::default()));
            }
            let (stream_status, _write_max_block_time_ms) = stream_status.unwrap();
            #[cfg(feature = "anyproxy-write-block-time-ms")]
            {
                if _write_max_block_time_ms > self.stream_info.borrow().write_max_block_time_ms {
                    self.stream_info.borrow_mut().write_max_block_time_ms =
                        _write_max_block_time_ms;
                }
            }

            if self.stream_info.borrow().debug_is_open_print {
                let stream_info = self.stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:read file stream_status:{:?}, stream_cache_size:{}, max_tmp_file_size:{}, tmp_file_size:{} ",
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
