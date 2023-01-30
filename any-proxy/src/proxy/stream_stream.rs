#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_var;
use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use super::StreamConfigContext;
use super::StreamLimit;
use super::StreamStreamContext;
use super::MIN_CACHE_BUFFER;
use super::MIN_CACHE_FILE;
use super::MIN_READ_BUFFER;
#[cfg(unix)]
use super::PAGE_SIZE;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::protopack;
use crate::proxy;
use crate::proxy::{StreamTimeout, MIN_MERGER_CACHE_BUFFER};
use crate::stream::stream_flow;
use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use crate::util::var;
use anyhow::Result;
use anyhow::{anyhow, Error};
use chrono::Local;
use dynamic_pool::{DynamicPool, DynamicPoolItem};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use tempfile::NamedTempFile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const LIMIT_SLEEP_TIME_MILLIS: u64 = 100;
const NORMAL_SLEEP_TIME_MILLIS: u64 = 1000 * 5;
const NOT_SLEEP_TIME_MILLIS: u64 = 0;
const MIN_SLEEP_TIME_MILLIS: u64 = 10;

#[derive(Debug)]
pub enum StreamStatus {
    Limit,
    Full(u64),
    Ok(u64),
}

pub struct StreamStream {}

impl StreamStream {
    pub fn work_time(name: &str, stream_work_times: bool, stream_info: &mut StreamInfo) {
        if stream_work_times {
            let stream_work_time = stream_info
                .stream_work_time
                .as_ref()
                .unwrap()
                .elapsed()
                .as_secs_f32();
            stream_info
                .stream_work_times
                .push((name.to_string(), stream_work_time));
            stream_info.stream_work_time = Some(Instant::now());
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
        client_local_addr: &SocketAddr,
        client_remote_addr: &SocketAddr,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    ) -> Result<()> {
        StreamStream::work_time(
            "connect",
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

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
            StreamStream::work_time(
                "ebpf",
                config_ctx.stream.stream_work_times,
                &mut stream_info.borrow_mut(),
            );

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

        StreamStream::work_time(
            "stream_to_stream",
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

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

        StreamStream::work_time(
            "end",
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

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
                    "client -> upstream",
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
                    "upstream -> client",
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
        flag: &str,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 || config_ctx.stream.stream_cache_size > 0 {
            StreamStream::do_stream_to_file_stream(
                limit,
                config_ctx,
                stream_info,
                r,
                w,
                #[cfg(unix)]
                sendfile,
                flag,
                thread_id,
                tmp_file_id,
            )
            .await
        } else {
            StreamStream::stream_to_cache_stream(limit, config_ctx, stream_info, r, w, flag).await
        }
    }

    pub async fn stream_to_cache_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: R,
        mut w: W,
        flag: &str,
    ) -> Result<()> {
        stream_info.borrow_mut().buffer_cache = Some("memory".to_string());
        StreamStream::work_time(
            &format!("{} cache", flag),
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

        let mut ssc = StreamStreamContext {
            stream_cache_size: 0,
            tmp_file_size: 0,
            limit_rate_after: limit.limit_rate_after,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: 0,
            max_tmp_file_size: 0,
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

            if stream_info.borrow().is_open_print {
                let stream_info = stream_info.borrow();
                log::info!(
                    "{}---{}---{}:read buffer.size:{}",
                    stream_info.request_id,
                    stream_info.local_addr,
                    flag,
                    buffer.size,
                );
            }

            if is_first {
                is_first = false;
                StreamStream::work_time(
                    flag,
                    config_ctx.stream.stream_work_times,
                    &mut stream_info.borrow_mut(),
                );
            }

            loop {
                if buffer.start >= buffer.size {
                    break;
                }

                if stream_info.borrow().is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{}---{}:write start:{}, size:{}",
                        stream_info.request_id,
                        stream_info.local_addr,
                        flag,
                        buffer.start,
                        buffer.size
                    );
                }

                let stream_status = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut ssc,
                    &mut w,
                    flag,
                    #[cfg(unix)]
                    &None,
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;
                if stream_info.borrow().is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{}---{}: stream_status:{:?}",
                        stream_info.request_id,
                        stream_info.local_addr,
                        flag,
                        stream_status
                    );
                }

                if let StreamStatus::Limit = stream_status {
                    tokio::time::sleep(std::time::Duration::from_millis(LIMIT_SLEEP_TIME_MILLIS))
                        .await;
                }
            }
        }
    }

    pub async fn stream_limit_write<W: AsyncWrite + Unpin>(
        buffer: &mut StreamCacheBuffer,
        ssc: &mut StreamStreamContext,
        w: &mut W,
        flag: &str,
        #[cfg(unix)] sendfile: &Option<SendFile>,
    ) -> Result<StreamStatus> {
        let mut n = buffer.size - buffer.start;
        let curr_limit_rate = ssc.curr_limit_rate.load(Ordering::Relaxed);
        let limit_rate_size = if ssc.max_limit_rate <= 0 {
            n
        } else if ssc.limit_rate_after > 0 {
            ssc.limit_rate_after
        } else if curr_limit_rate > 0 {
            curr_limit_rate
        } else {
            return Ok(StreamStatus::Limit);
        };

        if limit_rate_size < n {
            n = limit_rate_size;
        }
        log::trace!("{}, n:{}", flag, n);

        let wn = loop {
            #[cfg(unix)]
            if sendfile.is_some() && buffer.file_fd > 0 {
                let wn = sendfile
                    .as_ref()
                    .unwrap()
                    .write(buffer.file_fd, buffer.seek, n)
                    .await
                    .map_err(|e| anyhow!("err:w.write_all => e:{}", e))?;
                break wn;
            }

            let end = buffer.start + n;
            let wn = w
                .write(&buffer.datas[buffer.start as usize..end as usize])
                .await
                .map_err(|e| anyhow!("err:w.write_all => e:{}", e))? as u64;

            break wn;
        };

        log::trace!("{}, wn:{}", flag, wn);
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
            return Ok(StreamStatus::Full(wn));
        }
        return Ok(StreamStatus::Ok(wn));
    }

    pub async fn do_stream_to_file_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: R,
        mut w: W,
        #[cfg(unix)] sendfile: Option<SendFile>,
        flag: &str,
        _thread_id: std::thread::ThreadId,
        _tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 && config_ctx.stream.stream_cache_size > 0 {
            stream_info.borrow_mut().buffer_cache = Some("file_and_cache".to_string());
        } else if limit.tmp_file_size > 0 && config_ctx.stream.stream_cache_size <= 0 {
            stream_info.borrow_mut().buffer_cache = Some("file".to_string());
        } else if limit.tmp_file_size <= 0 && config_ctx.stream.stream_cache_size > 0 {
            stream_info.borrow_mut().buffer_cache = Some("cache".to_string());
        }

        StreamStream::work_time(
            &format!("{} file", flag),
            config_ctx.stream.stream_work_times,
            &mut stream_info.borrow_mut(),
        );

        let maximum_capacity = std::cmp::max(
            config_ctx.stream.stream_cache_size as u64,
            MIN_MERGER_CACHE_BUFFER * 32,
        );

        let initial_capacity = 8;
        let maximum_capacity =
            (maximum_capacity / MIN_MERGER_CACHE_BUFFER) as usize + initial_capacity;
        let buffer_pool = DynamicPool::new(
            initial_capacity,
            maximum_capacity,
            StreamCacheBuffer::default,
        );

        let mut caches: VecDeque<StreamCache> = VecDeque::new();

        let fw = NamedTempFile::new().map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
        let fr = fw
            .reopen()
            .map_err(|e| anyhow!("err:fw.reopen => e:{}", e))?;

        let fw = Arc::new(Mutex::new(fw));
        let mut fw_seek: u64 = 0;

        #[cfg(unix)]
        let fr_fd = fr.as_raw_fd();

        let fr = Arc::new(Mutex::new(fr));

        let mut ssc = StreamStreamContext {
            stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            tmp_file_size: limit.tmp_file_size,
            limit_rate_after: limit.limit_rate_after,
            max_limit_rate: limit.max_limit_rate,
            curr_limit_rate: limit.curr_limit_rate.clone(),
            max_stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            max_tmp_file_size: limit.tmp_file_size,
        };

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size < MIN_CACHE_BUFFER as u64 {
            ssc.stream_cache_size = MIN_CACHE_BUFFER as u64;
            ssc.max_stream_cache_size = MIN_CACHE_BUFFER as u64;
        }

        if ssc.tmp_file_size > 0 && ssc.tmp_file_size < MIN_CACHE_FILE as u64 {
            ssc.tmp_file_size = MIN_CACHE_FILE as u64;
            ssc.max_tmp_file_size = MIN_CACHE_FILE as u64;
        }

        let mut is_first = true;
        let mut left_buffer: Option<DynamicPoolItem<StreamCacheBuffer>> = None;
        let mut read_buffer: Option<DynamicPoolItem<StreamCacheBuffer>> = None;
        let mut read_err: Option<Result<()>> = None;
        let mut stream_status = StreamStatus::Ok(0);
        loop {
            if stream_info.borrow().is_open_print {
                let stream_info = stream_info.borrow();
                log::info!(
                    "{}---{}---{}:read file stream_cache_size:{}, tmp_file_size:{}, read_err.is_none:{}",
                    stream_info.request_id,
                    stream_info.local_addr,
                    flag,
                    ssc.stream_cache_size,
                    ssc.tmp_file_size,
                    read_err.is_none(),
                );
            }

            if (ssc.stream_cache_size > 0 || ssc.tmp_file_size > 0) && read_err.is_none() {
                let mut buffer = if read_buffer.is_some() {
                    read_buffer.take().unwrap()
                } else {
                    buffer_pool.take()
                };

                buffer.resize(None);
                buffer.is_cache = false;

                if buffer.size < buffer.read_size {
                    log::error!("err:buffer.size < buffer.read_size");
                    return Err(anyhow!("err:buffer.size < buffer.read_size"));
                }

                if ssc.stream_cache_size < buffer.read_size && ssc.tmp_file_size < buffer.read_size
                {
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
                        match stream_status {
                            StreamStatus::Limit => {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    LIMIT_SLEEP_TIME_MILLIS,
                                ))
                                    .await;
                            }
                            StreamStatus::Full(_) => {
                                log::error!("err:StreamStatus::Full");
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    LIMIT_SLEEP_TIME_MILLIS,
                                ))
                                    .await;
                            }
                            StreamStatus::Ok(_) => {
                                log::error!("err:StreamStatus::Ok");
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    LIMIT_SLEEP_TIME_MILLIS,
                                ))
                                    .await;
                            }
                        }
                        return Ok(0);
                    }
                    let sleep_read_time_millis =  if caches.len() > 0 || left_buffer.is_some() {
                        match stream_status {
                            StreamStatus::Limit => {
                                LIMIT_SLEEP_TIME_MILLIS
                            }
                            StreamStatus::Full(_) => {
                                NOT_SLEEP_TIME_MILLIS
                            }
                            StreamStatus::Ok(_) => {
                                NOT_SLEEP_TIME_MILLIS
                            }
                        }
                    } else {
                        if buffer.read_size > 0  {
                            MIN_SLEEP_TIME_MILLIS
                        }else {
                            NORMAL_SLEEP_TIME_MILLIS
                        }
                    };

                    let read_size = buffer.read_size as usize;
                    let end_size = buffer.size as usize;
                    tokio::select! {
                        biased;
                        ret = r.read(&mut buffer.datas[read_size..end_size]) => {
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

                if is_first {
                    is_first = false;
                    StreamStream::work_time(
                        flag,
                        config_ctx.stream.stream_work_times,
                        &mut stream_info.borrow_mut(),
                    );
                }

                let ret: Result<()> = async {
                    if let Err(ref e) = ret {
                        log::debug!("{}, read_err = Some(ret)", flag);
                        if e.kind() != std::io::ErrorKind::ConnectionReset {
                            return Err(anyhow!("err:{}", e))?;
                        }
                        read_err = Some(Err(anyhow!("err:{}", e)));
                        if buffer.read_size == 0 {
                            return Ok(());
                        }
                    } else {
                        let size = ret? as u64;
                        let old_read_size = buffer.read_size;
                        buffer.read_size += size;
                        log::trace!(
                            "{}, read size:{}, buffer.read_size:{}",
                            flag,
                            size,
                            buffer.read_size
                        );

                        if buffer.size != buffer.read_size {
                            if caches.len() > 0 || left_buffer.is_some() {
                                if buffer.read_size < MIN_READ_BUFFER as u64 {
                                    read_buffer = Some(buffer);
                                    return Ok(());
                                }
                            } else {
                                if old_read_size <= 0 && buffer.read_size < MIN_READ_BUFFER as u64 {
                                    read_buffer = Some(buffer);
                                    return Ok(());
                                }
                            }
                        }
                    }
                    buffer.size = buffer.read_size;
                    let size = buffer.size;
                    if stream_info.borrow().is_open_print {
                        let stream_info = stream_info.borrow();
                        log::info!(
                            "{}---{}---{}:read file is_cache:{}, size:{}",
                            stream_info.request_id,
                            stream_info.local_addr,
                            flag,
                            buffer.is_cache,
                            size,
                        );
                    }

                    if buffer.is_cache {
                        ssc.stream_cache_size -= size;
                        let last_cache = caches.back_mut();
                        if last_cache.is_some() {
                            let last_cache = last_cache.unwrap();
                            if let StreamCache::Buffer(last_cache) = last_cache {
                                if last_cache.size < MIN_MERGER_CACHE_BUFFER
                                    || buffer.size < MIN_MERGER_CACHE_BUFFER
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
                        caches.push_back(StreamCache::Buffer(buffer));
                        return Ok(());
                    }
                    ssc.tmp_file_size -= size;
                    let fw = fw.clone();
                    let ret: std::result::Result<(), Error> =
                        tokio::task::spawn_blocking(move || {
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

                    #[cfg(unix)]
                    if sendfile.is_some() {
                        let mut is_first = true;
                        let mut size = size;
                        loop {
                            if size <= 0 {
                                break;
                            }

                            let w_size = if is_first {
                                PAGE_SIZE as u64 - fw_seek % PAGE_SIZE as u64
                            } else {
                                PAGE_SIZE as u64
                            };
                            let w_size = if w_size > size { size } else { w_size };

                            let seek = fw_seek;
                            fw_seek += w_size;
                            size -= w_size;

                            if is_first {
                                is_first = false;
                                if w_size != PAGE_SIZE as u64 {
                                    let last_cache = caches.back_mut();
                                    if last_cache.is_some() {
                                        let last_cache = last_cache.unwrap();
                                        if let StreamCache::File(last_cache) = last_cache {
                                            last_cache.size += w_size;
                                            continue;
                                        }
                                    }
                                }
                            }
                            caches.push_back(StreamCache::File(StreamCacheFile {
                                seek,
                                size: w_size,
                            }));
                        }

                        return Ok(());
                    }

                    let seek = fw_seek;
                    fw_seek += size;

                    let last_cache = caches.back_mut();
                    if last_cache.is_some() {
                        let last_cache = last_cache.unwrap();
                        if let StreamCache::File(last_cache) = last_cache {
                            if last_cache.size < MIN_MERGER_CACHE_BUFFER
                                || size < MIN_MERGER_CACHE_BUFFER
                            {
                                last_cache.size += size;
                                return Ok(());
                            }
                        }
                    }

                    caches.push_back(StreamCache::File(StreamCacheFile { seek, size }));

                    return Ok(());
                }
                .await;
                let _ = ret?;
            } else {
                match stream_status {
                    StreamStatus::Limit => {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            LIMIT_SLEEP_TIME_MILLIS,
                        ))
                        .await;
                    }
                    StreamStatus::Full(_) => {
                        log::error!("err:StreamStatus::Full");
                        tokio::time::sleep(std::time::Duration::from_millis(
                            LIMIT_SLEEP_TIME_MILLIS,
                        ))
                        .await;
                    }
                    StreamStatus::Ok(_) => {
                        log::error!("err:StreamStatus::Ok");
                        tokio::time::sleep(std::time::Duration::from_millis(
                            LIMIT_SLEEP_TIME_MILLIS,
                        ))
                        .await;
                    }
                }
            }

            loop {
                let mut buffer = if left_buffer.is_some() {
                    log::trace!("{}, left_buffer", flag);
                    left_buffer.take().unwrap()
                } else {
                    log::trace!("{}, caches", flag);
                    let cache = caches.pop_front();
                    if cache.is_none() {
                        if read_err.is_some() {
                            log::trace!("{}, read_err.is_some() return", flag);
                            return read_err.take().unwrap();
                        }
                        break;
                    }
                    let cache = cache.unwrap();

                    let ret: std::result::Result<DynamicPoolItem<StreamCacheBuffer>, Error> =
                        match cache {
                            StreamCache::Buffer(mut buffer) => {
                                buffer.is_cache = true;
                                Ok(buffer)
                            }
                            StreamCache::File(file) => {
                                let mut buffer = buffer_pool.take();
                                buffer.resize(Some(file.size as usize));
                                buffer.size = file.size;
                                buffer.seek = file.seek;

                                let ret = loop {
                                    #[cfg(unix)]
                                    if sendfile.is_some() {
                                        buffer.file_fd = fr_fd;
                                        break Ok(buffer);
                                    }

                                    let fr = fr.clone();
                                    let ret: std::result::Result<
                                        DynamicPoolItem<StreamCacheBuffer>,
                                        Error,
                                    > = tokio::task::spawn_blocking(move || {
                                        let end_size = buffer.size as usize;
                                        let ret = fr
                                            .lock()
                                            .unwrap()
                                            .read_exact(&mut buffer.datas[..end_size])
                                            .map_err(|e| {
                                                anyhow!(
                                                    "err:fr.read_exact => e:{}, size:{}",
                                                    e,
                                                    file.size
                                                )
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
                        Ok(buffer) => buffer,
                        Err(err) => return Err(err)?,
                    }
                };

                log::trace!("{}, start:{}, size:{}", flag, buffer.start, buffer.size);

                if stream_info.borrow().is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{}---{}:read file buffer.start:{}, buffer.size:{}",
                        stream_info.request_id,
                        stream_info.local_addr,
                        flag,
                        buffer.start,
                        buffer.size,
                    );
                }

                stream_status = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut ssc,
                    &mut w,
                    flag,
                    #[cfg(unix)]
                    &sendfile,
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;

                if stream_info.borrow().is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{}---{}:read file stream_status:{:?}, stream_cache_size:{}, max_tmp_file_size:{}, tmp_file_size:{} ",
                        stream_info.request_id,
                        stream_info.local_addr,
                        flag,
                        stream_status,
                        ssc.stream_cache_size,
                        ssc.max_tmp_file_size,
                        ssc.tmp_file_size,
                    );
                }

                if buffer.start < buffer.size {
                    left_buffer = Some(buffer);
                }

                match stream_status {
                    StreamStatus::Limit => {
                        break;
                    }
                    StreamStatus::Full(_) => {
                        break;
                    }
                    StreamStatus::Ok(_) => {
                        //
                    }
                }
            }
        }
    }

    pub async fn stream_info_parse(
        stream_timeout: StreamTimeout,
        stream_info: &mut StreamInfo,
    ) -> Result<bool> {
        let mut is_close = false;
        if stream_info.err_status == ErrStatus::Ok {
            let mut client_stream_flow_info = stream_info.client_stream_flow_info.lock().unwrap();
            let mut upstream_stream_flow_info =
                stream_info.upstream_stream_flow_info.lock().unwrap();

            if stream_timeout.timeout_num.load(Ordering::Relaxed) >= 4 {
                let client_read_timeout_millis = stream_timeout
                    .client_read_timeout_millis
                    .load(Ordering::Relaxed);
                let client_write_timeout_millis = stream_timeout
                    .client_write_timeout_millis
                    .load(Ordering::Relaxed);
                let (client_err, mut client_timeout_millis) =
                    if client_stream_flow_info.read == client_stream_flow_info.write {
                        (
                            stream_flow::StreamFlowErr::ReadTimeout,
                            client_read_timeout_millis,
                        )
                    } else {
                        (
                            stream_flow::StreamFlowErr::WriteTimeout,
                            client_write_timeout_millis,
                        )
                    };

                if client_stream_flow_info.err != stream_flow::StreamFlowErr::Init {
                    client_timeout_millis = 0;
                }

                let ups_read_timeout_millis = stream_timeout
                    .ups_read_timeout_millis
                    .load(Ordering::Relaxed);
                let ups_write_timeout_millis = stream_timeout
                    .ups_write_timeout_millis
                    .load(Ordering::Relaxed);
                let (ups_err, mut ups_timeout_millis) =
                    if upstream_stream_flow_info.read == upstream_stream_flow_info.write {
                        (
                            stream_flow::StreamFlowErr::ReadTimeout,
                            ups_read_timeout_millis,
                        )
                    } else {
                        (
                            stream_flow::StreamFlowErr::WriteTimeout,
                            ups_write_timeout_millis,
                        )
                    };

                if upstream_stream_flow_info.err != stream_flow::StreamFlowErr::Init {
                    ups_timeout_millis = 0;
                }

                if client_timeout_millis > 0 || ups_timeout_millis > 0 {
                    if client_timeout_millis > ups_timeout_millis {
                        client_stream_flow_info.err = client_err;
                        client_stream_flow_info.err_time_millis = client_timeout_millis;
                    } else {
                        upstream_stream_flow_info.err = ups_err;
                        upstream_stream_flow_info.err_time_millis = ups_timeout_millis;
                    }
                }
            }

            let (err, err_status_200): (
                stream_flow::StreamFlowErr,
                Option<Box<dyn stream_info::ErrStatus200>>,
            ) = if client_stream_flow_info.err != stream_flow::StreamFlowErr::Init
                && upstream_stream_flow_info.err != stream_flow::StreamFlowErr::Init
            {
                if client_stream_flow_info.err_time_millis
                    >= upstream_stream_flow_info.err_time_millis
                {
                    (
                        client_stream_flow_info.err.clone(),
                        Some(Box::new(stream_info::ErrStatusClient {})),
                    )
                } else {
                    (
                        upstream_stream_flow_info.err.clone(),
                        Some(Box::new(stream_info::ErrStatusUpstream {})),
                    )
                }
            } else if client_stream_flow_info.err != stream_flow::StreamFlowErr::Init {
                (
                    client_stream_flow_info.err.clone(),
                    Some(Box::new(stream_info::ErrStatusClient {})),
                )
            } else if upstream_stream_flow_info.err != stream_flow::StreamFlowErr::Init {
                (
                    upstream_stream_flow_info.err.clone(),
                    Some(Box::new(stream_info::ErrStatusUpstream {})),
                )
            } else {
                (stream_flow::StreamFlowErr::Init, None)
            };

            if err != stream_flow::StreamFlowErr::Init {
                let err_status_200 = err_status_200.as_ref().unwrap();
                if err == stream_flow::StreamFlowErr::WriteClose {
                    is_close = true;
                    stream_info.err_status_str = Some(err_status_200.write_close());
                } else if err == stream_flow::StreamFlowErr::ReadClose {
                    is_close = true;
                    stream_info.err_status_str = Some(err_status_200.read_close());
                } else if err == stream_flow::StreamFlowErr::WriteReset {
                    is_close = true;
                    stream_info.err_status_str = Some(err_status_200.write_reset());
                } else if err == stream_flow::StreamFlowErr::ReadReset {
                    is_close = true;
                    stream_info.err_status_str = Some(err_status_200.read_reset());
                } else if err == stream_flow::StreamFlowErr::WriteTimeout {
                    stream_info.err_status_str = Some(err_status_200.write_timeout());
                } else if err == stream_flow::StreamFlowErr::ReadTimeout {
                    stream_info.err_status_str = Some(err_status_200.read_timeout());
                } else if err == stream_flow::StreamFlowErr::WriteErr {
                    stream_info.err_status_str = Some(err_status_200.write_err());
                } else if err == stream_flow::StreamFlowErr::ReadErr {
                    stream_info.err_status_str = Some(err_status_200.read_err());
                }
            }
        } else if stream_info.err_status == ErrStatus::ServiceUnavailable {
            let upstream_connect_flow_info = stream_info.upstream_connect_flow_info.borrow();
            if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteTimeout {
                stream_info.err_status = ErrStatus::GatewayTimeout;
            } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteReset {
                stream_info.err_status_str = Some(stream_info::UPS_CONN_RESET.to_string());
            } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteErr {
                stream_info.err_status_str = Some(stream_info::UPS_CONN_ERR.to_string());
            }
        }

        Ok(is_close)
    }

    pub async fn access_log(
        access: &Vec<config_toml::AccessConfig>,
        access_context: &Vec<proxy::proxy::AccessContext>,
        stream_var: &Rc<stream_var::StreamVar>,
        stream_info: &mut StreamInfo,
    ) -> Result<()> {
        for (index, access) in access.iter().enumerate() {
            if access.access_log {
                let access_context = &access_context[index];
                let mut access_format_var = var::Var::copy(&access_context.access_format_vars)
                    .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
                access_format_var.for_each(|var| {
                    let var_name = var::Var::var_name(var);
                    let value = stream_var.find(var_name, stream_info);
                    match value {
                        Err(e) => {
                            log::error!("{}", anyhow!("{}", e));
                            Ok(None)
                        }
                        Ok(value) => Ok(value),
                    }
                })?;

                let mut access_log_data = access_format_var
                    .join()
                    .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;

                if access.access_log_stdout {
                    log::info!("{}", access_log_data);
                }
                access_log_data.push_str("\n");
                let access_log_file = access_context.access_log_file.clone();
                tokio::task::spawn_blocking(move || {
                    let mut access_log_file = access_log_file.as_ref();
                    let ret = access_log_file
                        .write_all(access_log_data.as_bytes())
                        .map_err(|e| {
                            anyhow!(
                                "err:access_log_file.write_all => access_log_data:{}, e:{}",
                                access_log_data,
                                e
                            )
                        });
                    if let Err(e) = ret {
                        log::error!("{}", e);
                    }
                });
            }
        }
        Ok(())
    }

    pub async fn debug_access_log(
        access: &Vec<config_toml::AccessConfig>,
        access_context: &Vec<proxy::proxy::AccessContext>,
        stream_var: &Rc<stream_var::StreamVar>,
        stream_info: &mut StreamInfo,
    ) -> Result<()> {
        for (index, _) in access.iter().enumerate() {
            let access_context = &access_context[index];
            let mut access_format_var = var::Var::copy(&access_context.access_format_vars)
                .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            access_format_var.for_each(|var| {
                let var_name = var::Var::var_name(var);
                let value = stream_var.find(var_name, stream_info);
                match value {
                    Err(e) => {
                        log::error!("{}", anyhow!("{}", e));
                        Ok(None)
                    }
                    Ok(value) => Ok(value),
                }
            })?;

            let access_log_data = access_format_var
                .join()
                .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
            log::info!("***debug***{}", access_log_data);
        }
        Ok(())
    }

    pub async fn read_timeout(
        stream_flow_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
        stream_info: Rc<RefCell<StreamInfo>>,
        stream_timeout: StreamTimeout,
        timeout_millis: Arc<AtomicI64>,
    ) -> Result<()> {
        let (mut read_timeout, mut read) = {
            let stream_flow_info = stream_flow_info.lock().unwrap();
            let mut read_timeout = stream_flow_info.read_timeout;
            if read_timeout <= 0 || read_timeout == u64::MAX {
                read_timeout = 10;
            }
            let read = stream_flow_info.read;
            (read_timeout, read)
        };

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(read_timeout)).await;
            read_timeout = {
                let read_timeout = stream_flow_info.lock().unwrap().read_timeout;
                read_timeout
            };
            if read_timeout <= 0 || read_timeout == u64::MAX {
                read_timeout = 10;
                continue;
            }

            if stream_info.borrow().is_discard_timeout {
                continue;
            }

            let curr_read = { stream_flow_info.lock().unwrap().read };
            if curr_read <= read {
                let stream_flow_info = stream_flow_info.lock().unwrap();
                let time_millis = if stream_flow_info.err != StreamFlowErr::Init {
                    1 as i64
                } else {
                    Local::now().timestamp_millis()
                };

                if timeout_millis.load(Ordering::Relaxed) == 0 {
                    stream_timeout.timeout_num.fetch_add(1, Ordering::Relaxed);
                }
                timeout_millis.store(time_millis, Ordering::Relaxed);

                if stream_timeout.timeout_num.load(Ordering::Relaxed) >= 4 {
                    return Ok(());
                }
            } else {
                if timeout_millis.load(Ordering::Relaxed) != 0 {
                    stream_timeout.timeout_num.fetch_sub(1, Ordering::Relaxed);
                    timeout_millis.store(0, Ordering::Relaxed);
                }
            }
            read = curr_read;
        }
    }

    pub async fn write_timeout(
        stream_flow_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
        stream_info: Rc<RefCell<StreamInfo>>,
        stream_timeout: StreamTimeout,
        timeout_millis: Arc<AtomicI64>,
    ) -> Result<()> {
        let (mut write_timeout, mut write) = {
            let stream_flow_info = stream_flow_info.lock().unwrap();
            let mut write_timeout = stream_flow_info.write_timeout;
            if write_timeout <= 0 || write_timeout == u64::MAX {
                write_timeout = 10;
            }
            let write = stream_flow_info.write;
            (write_timeout, write)
        };

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(write_timeout)).await;
            write_timeout = {
                let write_timeout = stream_flow_info.lock().unwrap().write_timeout;
                write_timeout
            };
            if write_timeout <= 0 || write_timeout == u64::MAX {
                write_timeout = 10;
                continue;
            }

            if stream_info.borrow().is_discard_timeout {
                continue;
            }

            let curr_write = { stream_flow_info.lock().unwrap().write };
            if curr_write <= write {
                let stream_flow_info = stream_flow_info.lock().unwrap();
                let time_millis = if stream_flow_info.err != StreamFlowErr::Init {
                    1 as i64
                } else {
                    Local::now().timestamp_millis()
                };

                if timeout_millis.load(Ordering::Relaxed) == 0 {
                    stream_timeout.timeout_num.fetch_add(1, Ordering::Relaxed);
                }
                timeout_millis.store(time_millis, Ordering::Relaxed);

                if stream_timeout.timeout_num.load(Ordering::Relaxed) >= 4 {
                    return Ok(());
                }
            } else {
                if timeout_millis.load(Ordering::Relaxed) != 0 {
                    stream_timeout.timeout_num.fetch_sub(1, Ordering::Relaxed);
                    timeout_millis.store(0, Ordering::Relaxed);
                }
            }
            write = curr_write;
        }
    }

    pub async fn stream_work_debug(
        stream_work_debug_time: u64,
        stream_info: Rc<RefCell<StreamInfo>>,
    ) -> Result<()> {
        let timeout = if stream_work_debug_time <= 0 {
            10
        } else {
            stream_work_debug_time
        };

        let mut config_ctx: Option<Rc<StreamConfigContext>> = None;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
            if stream_work_debug_time <= 0 || stream_work_debug_time == u64::MAX {
                continue;
            }
            if stream_info.borrow().is_discard_flow {
                continue;
            }
            if stream_info.borrow().stream_config_context.is_some() {
                if config_ctx.is_none() {
                    config_ctx = stream_info.borrow().stream_config_context.clone();
                }
                let config_ctx = config_ctx.as_ref().unwrap();
                if let Err(e) = StreamStream::debug_access_log(
                    &config_ctx.access,
                    &config_ctx.access_context,
                    &config_ctx.stream_var,
                    &mut stream_info.borrow_mut(),
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream::debug_access_log => e:{}", e))
                {
                    log::error!("err:{}", e);
                    continue;
                }
            }
        }
    }
}
