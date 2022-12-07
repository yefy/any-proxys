use super::stream_info;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::stream_var;
use super::StreamCache;
use super::StreamCacheBuffer;
use super::StreamCacheFile;
use super::StreamConfigContext;
use super::StreamLimit;
use super::StreamLimitData;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::ebpf_sockhash;
use crate::io::buf_writer::BufWriter;
use crate::protopack;
use crate::proxy;
use crate::stream::stream_flow;
use crate::stream::stream_flow::StreamFlow;
use crate::util::var;
use anyhow::Result;
use anyhow::{anyhow, Error};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use std::vec;
use syncpool::prelude::*;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MIN_CACHE_BUFFER: usize = 8192 * 2;
const MIN_CACHE_FILE: usize = 1024 * 1024 * 10;

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
        mut client_stream: stream_flow::StreamFlow,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
        client_local_addr: &SocketAddr,
        client_remote_addr: &SocketAddr,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
    ) -> Result<()> {
        log::debug!("upstream connect");
        let (
            upstream_protocol_name,
            mut upstream_stream,
            upstream_addr,
            upstream_elapsed,
            ups_local_addr,
            ups_remote_addr,
        ) = {
            stream_info.borrow_mut().err_status = ErrStatus::ServiceUnavailable;
            let connect_func = {
                let ups_data = config_ctx.ups_data.borrow();
                ups_data.as_ref().unwrap().connect()
            };
            if connect_func.is_none() {
                return Err(anyhow!("err: connect_func nil"));
            }
            let connect_func = connect_func.unwrap();
            connect_func
                .connect(&mut Some(
                    &mut stream_info.borrow().upstream_connect_info.borrow_mut(),
                ))
                .await
                .map_err(|e| anyhow!("err:connect => e:{}", e))?
        };
        log::debug!(
            "upstream_protocol_name:{}",
            upstream_protocol_name.to_string()
        );
        {
            let mut stream_info = stream_info.borrow_mut();
            upstream_stream.set_stream_info(Some(stream_info.upstream_stream_info.clone()));
            stream_info.upstream_protocol_name = Some(upstream_protocol_name.to_string());
            stream_info.upstream_addr = Some(upstream_addr);
            stream_info.upstream_connect_time = Some(upstream_elapsed);
            StreamStream::work_time(
                "client::connect",
                config_ctx.stream.stream_work_times,
                &mut stream_info,
            );

            stream_info.err_status = ErrStatus::Ok;
            if config_ctx.proxy_protocol {
                let mut upstream_buf_writer = BufWriter::new(&upstream_stream);
                protopack::anyproxy::write_pack(
                    &mut upstream_buf_writer,
                    protopack::anyproxy::AnyproxyHeaderType::Hello,
                    stream_info.protocol_hello.as_ref().unwrap(),
                )
                .await
                .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
            }
        }

        if client_buffer.len() > 0 {
            upstream_stream
                .write(client_buffer)
                .await
                .map_err(|e| anyhow!("err:client_buf_reader.buffer => e:{}", e))?;
        }

        #[cfg(feature = "anyproxy-ebpf")]
        if config_ctx.fast.is_ebpf && client_stream.fd() > 0 && upstream_stream.fd() > 0 {
            stream_info.borrow_mut().is_ebpf = true;
            StreamStream::work_time(
                "ebpf start upstream_stream fd",
                config_ctx.stream.stream_work_times,
                &mut stream_info.borrow_mut(),
            );
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
            let mut rx = ebpf_add_sock_hash
                .add(
                    client_stream.fd(),
                    //-1,
                    client_remote_addr.clone(),
                    client_local_addr.clone(),
                    upstream_stream.fd(),
                    //-1,
                    ups_remote_addr,
                    ups_local_addr,
                )
                .await?;
            let _ = rx.recv().await;
            StreamStream::work_time(
                "ebpf end upstream_stream fd",
                config_ctx.stream.stream_work_times,
                &mut stream_info.borrow_mut(),
            );

            upstream_stream
                .write("".as_bytes())
                .await
                .map_err(|e| anyhow!("err:upstream_stream.write => e:{}", e))?;
            client_stream
                .write("".as_bytes())
                .await
                .map_err(|e| anyhow!("err:client_stream.write => e:{}", e))?;
        }

        // log::info!("stream_to_stream");
        let ret = StreamStream::stream_to_stream(
            config_ctx.clone(),
            stream_info.clone(),
            client_stream,
            upstream_stream,
            thread_id,
            tmp_file_id,
        )
        .await;

        StreamStream::work_time(
            "stream_to_stream",
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
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        let (client_read, client_write) = tokio::io::split(client);
        let (upstream_read, upstream_write) = tokio::io::split(upstream);
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
                ret = StreamStream::do_stream_to_stream(upload_limit, config_ctx.clone(), stream_info.clone(), client_read, upstream_write, "client -> upstream", thread_id.clone(), tmp_file_id.clone())  => {
                    return ret;
                }
                ret =  StreamStream::do_stream_to_stream(download_limit, config_ctx,stream_info,upstream_read,client_write,"upstream -> client", thread_id, tmp_file_id) => {
                    return ret;
                }
                else => {
                    return Err(anyhow!("err:stream_to_stream_or_file select close"));
                }
            }
        }.await;
        ret
    }

    pub async fn do_stream_to_stream(
        limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        r: tokio::io::ReadHalf<StreamFlow>,
        w: tokio::io::WriteHalf<StreamFlow>,
        flag: &str,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0 {
            StreamStream::do_stream_to_file_stream(
                limit,
                config_ctx,
                stream_info,
                r,
                w,
                flag,
                thread_id,
                tmp_file_id,
            )
            .await
        } else {
            StreamStream::stream_to_cache_stream(limit, config_ctx, stream_info, r, w, flag).await
        }
    }

    pub async fn stream_check_limit_rate(
        limit_data: &mut StreamLimitData,
        limit: &mut StreamLimit,
    ) -> bool {
        let elapsed_time = limit_data.start_time.elapsed().as_secs();
        if elapsed_time >= 1 {
            limit_data.start_time = Instant::now();
            limit_data.limit_rate = limit.limit_rate;
            true
        } else {
            false
        }
    }

    pub async fn stream_to_cache_stream(
        mut limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: tokio::io::ReadHalf<StreamFlow>,
        mut w: tokio::io::WriteHalf<StreamFlow>,
        flag: &str,
    ) -> Result<()> {
        let mut stream_cache_size = config_ctx.stream.stream_cache_size;
        if stream_cache_size < MIN_CACHE_BUFFER {
            stream_cache_size = MIN_CACHE_BUFFER;
        }
        let mut buffer = StreamCacheBuffer {
            data: vec![0u8; stream_cache_size],
            start: 0,
            size: 0,
            is_cache: true,
        };

        let mut limit_data = StreamLimitData {
            start_time: Instant::now(),
            stream_cache_size: 0,
            tmp_file_size: 0,
            limit_rate_after: limit.limit_rate_after,
            limit_rate: limit.limit_rate,
            max_stream_cache_size: 0,
            max_tmp_file_size: 0,
        };

        let mut is_first = true;
        loop {
            buffer.size = r
                .read(&mut buffer.data)
                .await
                .map_err(|e| anyhow!("err:r.read => e:{}", e))? as u64;
            buffer.start = 0;

            if is_first {
                is_first = false;
                StreamStream::work_time(
                    flag,
                    config_ctx.stream.stream_work_times,
                    &mut stream_info.borrow_mut(),
                );
            }

            //log::info!("{}, start:{}, size:{}", flag, buffer.start, buffer.size);

            loop {
                if buffer.start >= buffer.size {
                    break;
                }
                // log::info!("{}, start:{}, size:{}", flag, buffer.start, buffer.size);

                let (n, _) = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut limit_data,
                    &mut limit,
                    &mut w,
                    flag,
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;

                if n <= 0 {
                    loop {
                        let is_new_time =
                            StreamStream::stream_check_limit_rate(&mut limit_data, &mut limit)
                                .await;
                        if is_new_time {
                            break;
                        } else {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    }
                }
            }
        }
    }

    pub async fn stream_limit_write(
        buffer: &mut StreamCacheBuffer,
        limit_data: &mut StreamLimitData,
        limit: &mut StreamLimit,
        w: &mut tokio::io::WriteHalf<StreamFlow>,
        flag: &str,
    ) -> Result<(u64, bool)> {
        let mut limit_max_size = buffer.size - buffer.start;
        let limit_rate_size = if limit.limit_rate <= 0 {
            &mut limit_max_size
        } else if limit_data.limit_rate_after > 0 {
            &mut limit_data.limit_rate_after
        } else if limit_data.limit_rate > 0 {
            &mut limit_data.limit_rate
        } else {
            return Ok((0, false));
        };

        let mut n = buffer.size - buffer.start;
        if *limit_rate_size < n {
            n = *limit_rate_size;
        }
        log::trace!("{}, n:{}", flag, n);
        let end = buffer.start + n;
        let wn = w
            .write(&buffer.data[buffer.start as usize..end as usize])
            .await
            .map_err(|e| anyhow!("err:w.write_all => e:{}", e))? as u64;
        log::trace!("{}, wn:{}", flag, wn);
        *limit_rate_size -= wn;
        buffer.start += wn;
        if buffer.is_cache {
            if limit_data.max_stream_cache_size > 0 {
                limit_data.stream_cache_size += wn;
            }
        } else {
            if limit_data.max_tmp_file_size > 0 {
                limit_data.tmp_file_size += wn;
            }
        }
        Ok((wn, wn != n))
    }

    pub async fn do_stream_to_file_stream(
        mut limit: StreamLimit,
        config_ctx: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: tokio::io::ReadHalf<StreamFlow>,
        mut w: tokio::io::WriteHalf<StreamFlow>,
        flag: &str,
        _thread_id: std::thread::ThreadId,
        _tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        let mut pool = SyncPool::with_packer(|mut buffer: Box<StreamCacheBuffer>| {
            buffer.data = vec![0u8; MIN_CACHE_BUFFER];
            buffer.start = 0;
            buffer.size = 0;
            buffer.is_cache = false;
            buffer
        });
        let mut caches: Vec<StreamCache> = Vec::new();

        let fw = NamedTempFile::new().map_err(|e| anyhow!("err:NamedTempFile::new => e:{}", e))?;
        let fw = Arc::new(Mutex::new(fw));
        let fr = {
            fw.lock()
                .as_ref()
                .unwrap()
                .reopen()
                .map_err(|e| anyhow!("err:fw.reopen => e:{}", e))?
        };
        let fr = Arc::new(Mutex::new(fr));

        let mut limit_data = StreamLimitData {
            start_time: Instant::now(),
            stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            tmp_file_size: limit.tmp_file_size,
            limit_rate_after: limit.limit_rate_after,
            limit_rate: limit.limit_rate,
            max_stream_cache_size: config_ctx.stream.stream_cache_size as u64,
            max_tmp_file_size: limit.tmp_file_size,
        };

        if limit_data.stream_cache_size < MIN_CACHE_BUFFER as u64 {
            limit_data.stream_cache_size = MIN_CACHE_BUFFER as u64;
            limit_data.max_stream_cache_size = MIN_CACHE_BUFFER as u64;
        }

        if limit_data.tmp_file_size > 0 && limit_data.tmp_file_size < MIN_CACHE_FILE as u64 {
            limit_data.tmp_file_size = MIN_CACHE_FILE as u64;
            limit_data.max_tmp_file_size = MIN_CACHE_FILE as u64;
        }

        let mut is_first = true;
        let mut left_buffer: Option<Box<StreamCacheBuffer>> = None;
        let mut read_err: Option<Result<usize>> = None;
        loop {
            if (limit_data.stream_cache_size > 0 || limit_data.tmp_file_size > 0)
                && read_err.is_none()
            {
                let mut buffer = pool.get();
                buffer.start = 0;
                buffer.size = MIN_CACHE_BUFFER as u64;
                buffer.is_cache = false;

                if limit_data.stream_cache_size > 0 {
                    buffer.is_cache = true;
                    if limit_data.stream_cache_size < buffer.size {
                        buffer.size = limit_data.stream_cache_size;
                    }
                }

                if !buffer.is_cache {
                    if limit_data.tmp_file_size > 0 {
                        if limit_data.tmp_file_size < buffer.size {
                            buffer.size = limit_data.tmp_file_size;
                        }
                    }
                }

                let ret: Result<usize> = async {
                    tokio::select! {
                        biased;
                        ret = r.read(&mut buffer.data[..buffer.size as usize]) => {
                            let n = ret.map_err(|e| anyhow!("err:r.read => e:{}", e))?;
                            return Ok(n);
                        },
                        _= tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                            return Ok(0);
                        },
                        else => {
                            return Err(anyhow!(
                            "err:do_stream_to_stream_or_file select close"
                        ));
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

                if ret.is_err() {
                    log::trace!("{}, read_err = Some(ret)", flag);
                    read_err = Some(ret);
                    pool.put(buffer);
                } else {
                    let size = ret? as u64;
                    log::trace!("{}, read size:{}", flag, size);
                    buffer.size = size;
                    if size > 0 {
                        if buffer.is_cache {
                            limit_data.stream_cache_size -= size;
                            caches.push(StreamCache::Buffer(buffer));
                        } else {
                            limit_data.tmp_file_size -= size;
                            let fw = fw.clone();
                            let ret: std::result::Result<
                                Box<StreamCacheBuffer>,
                                (Box<StreamCacheBuffer>, Error),
                            > = tokio::task::spawn_blocking(move || {
                                let ret = fw
                                    .lock()
                                    .unwrap()
                                    .write_all(&buffer.data[..size as usize])
                                    .map_err(|e| {
                                        anyhow!("err:fw.write_all => e:{}, size:{}", e, size)
                                    });
                                match ret {
                                    Ok(_) => Ok(buffer),
                                    Err(err) => Err((buffer, err)),
                                }
                            })
                            .await?;

                            match ret {
                                Ok(buffer) => {
                                    caches.push(StreamCache::File(StreamCacheFile { size }));
                                    pool.put(buffer);
                                }
                                Err((buffer, err)) => {
                                    pool.put(buffer);
                                    Err(err)?
                                }
                            }
                        };
                    } else {
                        pool.put(buffer);
                    }
                }
            }

            loop {
                let mut buffer = if left_buffer.is_some() {
                    log::trace!("{}, left_buffer", flag);
                    left_buffer.take().unwrap()
                } else {
                    log::trace!("{}, caches", flag);
                    let cache = caches.pop();
                    if cache.is_none() {
                        if read_err.is_some() {
                            log::trace!("{}, read_err.is_some() return", flag);
                            read_err.take().unwrap()?;
                        }
                        break;
                    }
                    let cache = cache.unwrap();
                    match cache {
                        StreamCache::Buffer(mut buffer) => {
                            buffer.is_cache = true;
                            buffer
                        }
                        StreamCache::File(file) => {
                            let mut buffer = pool.get();
                            buffer.start = 0;
                            buffer.is_cache = false;
                            buffer.size = file.size;
                            let fr = fr.clone();
                            let ret: std::result::Result<
                                Box<StreamCacheBuffer>,
                                (Box<StreamCacheBuffer>, Error),
                            > = tokio::task::spawn_blocking(move || {
                                let ret = fr
                                    .lock()
                                    .unwrap()
                                    .read_exact(&mut buffer.data[..buffer.size as usize])
                                    .map_err(|e| {
                                        anyhow!("err:fr.read_exact => e:{}, size:{}", e, file.size)
                                    });
                                match ret {
                                    Ok(_) => return Ok(buffer),
                                    Err(err) => return Err((buffer, err)),
                                }
                            })
                            .await?;
                            match ret {
                                Ok(buffer) => buffer,
                                Err((buffer, err)) => {
                                    pool.put(buffer);
                                    Err(err)?
                                }
                            }
                        }
                    }
                };

                log::trace!("{}, start:{}, size:{}", flag, buffer.start, buffer.size);

                let (n, is_full) = StreamStream::stream_limit_write(
                    &mut buffer,
                    &mut limit_data,
                    &mut limit,
                    &mut w,
                    flag,
                )
                .await
                .map_err(|e| anyhow!("err:StreamStream.stream_write => e:{}", e))?;

                if buffer.start < buffer.size {
                    left_buffer = Some(buffer);
                } else {
                    pool.put(buffer);
                }

                let mut is_check_sleep = false;
                if n <= 0 {
                    if !StreamStream::stream_check_limit_rate(&mut limit_data, &mut limit).await {
                        is_check_sleep = true;
                    }
                } else {
                    if is_full {
                        is_check_sleep = true;
                    }
                }

                if is_check_sleep {
                    if limit_data.stream_cache_size <= 0
                        && limit_data.max_tmp_file_size > 0
                        && limit_data.tmp_file_size < 1024
                    {
                        log::trace!("{}, sleep", flag);
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    pub async fn stream_info_parse(stream_info: &mut StreamInfo) -> Result<bool> {
        let mut is_close = false;
        if stream_info.err_status == ErrStatus::Ok {
            let client_stream_info = stream_info.client_stream_info.lock().unwrap();
            let upstream_stream_info = stream_info.upstream_stream_info.lock().unwrap();

            let (err, err_status_200): (
                stream_flow::StreamFlowErr,
                Option<Box<dyn stream_info::ErrStatus200>>,
            ) = if client_stream_info.err != stream_flow::StreamFlowErr::Init {
                (
                    client_stream_info.err.clone(),
                    Some(Box::new(stream_info::ErrStatusClient {})),
                )
            } else if upstream_stream_info.err != stream_flow::StreamFlowErr::Init {
                (
                    upstream_stream_info.err.clone(),
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
            let upstream_connect_info = stream_info.upstream_connect_info.borrow();
            if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteTimeout {
                stream_info.err_status = ErrStatus::GatewayTimeout;
            } else if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteReset {
                stream_info.err_status_str = Some(stream_info::UPS_CONN_RESET.to_string());
            } else if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteErr {
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
                    let access_log_file = &mut access_log_file.as_ref();
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
}
