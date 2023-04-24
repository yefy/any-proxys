#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info::StreamInfo;
use super::StreamCacheBuffer;
use super::StreamConfigContext;
use super::StreamLimit;
use super::StreamStreamContext;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::protopack;
use crate::proxy::stream_stream_cache_or_file::StreamStreamCacheOrFile;
use crate::proxy::stream_stream_memory::StreamStreamMemory;
use crate::proxy::util as proxy_util;
use crate::stream::stream_flow;
use any_base::io::async_read_msg::AsyncReadMsg;
use any_base::io::async_stream::{AsyncStream, AsyncStreamExt};
use any_base::io::async_write_msg::AsyncWriteMsg;
use any_base::io::async_write_msg::AsyncWriteMsgExt;
use any_base::stream_flow::{StreamFlowRead, StreamFlowWrite};
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::atomic::Ordering;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct StreamFullInfo {
    pub is_sendfile: bool,
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
        stream_config_context: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_buffer: &[u8],
        #[cfg(feature = "anyproxy-ebpf")] mut client_stream: stream_flow::StreamFlow,
        #[cfg(not(feature = "anyproxy-ebpf"))] client_stream: stream_flow::StreamFlow,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
        client_local_addr: SocketAddr,
        client_remote_addr: SocketAddr,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: std::sync::Arc<any_ebpf::AddSockHash>,
    ) -> Result<()> {
        let (is_proxy_protocol_hello, mut upstream_stream) =
            proxy_util::upsteam_connect(stream_info.clone(), &stream_config_context).await?;

        let hello = proxy_util::get_proxy_hello(
            is_proxy_protocol_hello,
            stream_info.clone(),
            &stream_config_context,
        )
        .await;

        if hello.is_some() {
            stream_info.borrow_mut().protocol_hello_size = protopack::anyproxy::write_pack(
                &mut upstream_stream,
                protopack::anyproxy::AnyproxyHeaderType::Hello,
                &*hello.unwrap(),
            )
            .await
            .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
        }

        if client_buffer.len() > 0 {
            log::trace!("write:{:?}", &client_buffer[0..client_buffer.len()]);
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

        #[cfg(feature = "anyproxy-ebpf")]
        let ups_remote_addr = stream_info
            .borrow()
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .remote_addr
            .clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let ups_local_addr = stream_info
            .borrow()
            .upstream_connect_info
            .as_ref()
            .unwrap()
            .local_addr
            .clone();

        let mut _is_open_ebpf = false;
        #[cfg(feature = "anyproxy-ebpf")]
        if stream_config_context.fast_conf.is_open_ebpf
            && client_stream.fd() > 0
            && upstream_stream.fd() > 0
        {
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
            if stream_config_context.fast_conf.is_open_sendfile && !_is_open_ebpf {
                let client_sendfile = if _client_stream_fd > 0 {
                    Some(SendFile::new(
                        _client_stream_fd,
                        Some(stream_info.borrow().client_stream_flow_info.clone()),
                    ))
                } else {
                    None
                };

                let upstream_sendfile = if _upstream_stream_fd > 0 {
                    Some(SendFile::new(
                        _upstream_stream_fd,
                        Some(stream_info.borrow().upstream_stream_flow_info.clone()),
                    ))
                } else {
                    None
                };
                (client_sendfile, upstream_sendfile)
            } else {
                (None, None)
            };

        // log::info!("stream_to_stream");
        let ret = StreamStream::stream_to_stream(
            stream_config_context.clone(),
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
        .await
        .map_err(|e| {
            anyhow!(
                "err:stream_to_stream => request_id:{}, e:{}",
                stream_info.borrow().request_id,
                e
            )
        });

        #[cfg(feature = "anyproxy-ebpf")]
        if stream_config_context.fast_conf.is_open_ebpf
            && _client_stream_fd > 0
            && _upstream_stream_fd > 0
        {
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
        stream_config_context: Rc<StreamConfigContext>,
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
            stream_config_context.tmp_file.download_tmp_file_size,
            stream_config_context.rate.download_limit_rate_after,
            stream_config_context.rate.download_limit_rate,
        );
        let upload_limit = StreamLimit::new(
            stream_config_context.tmp_file.upload_tmp_file_size,
            stream_config_context.rate.upload_limit_rate_after,
            stream_config_context.rate.upload_limit_rate,
        );

        let ret: Result<()> = async {
            tokio::select! {
                _ = StreamStream::limit_timeout(upload_limit.clone()) => {
                    Ok(())
                }
                _ = StreamStream::limit_timeout(download_limit.clone()) => {
                    Ok(())
                }
                ret = StreamStream::do_stream_to_stream(
                    upload_limit,
                    stream_config_context.clone(),
                    stream_info.clone(),
                    client_read,
                    upstream_write,
                    #[cfg(unix)] upstream_sendfile,
                    true,
                    thread_id.clone(),
                    tmp_file_id.clone(),
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                ret =  StreamStream::do_stream_to_stream(
                    download_limit,
                    stream_config_context,
                    stream_info,
                    upstream_read,
                    client_write,
                    #[cfg(unix)] client_sendfile,
                    false,
                    thread_id,
                    tmp_file_id,
                ) => {
                    return ret.map_err(|e| anyhow!("err:ups => e:{}", e));
                }
                else => {
                    return Err(anyhow!("err:stream_to_stream_or_file select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn stream_to_stream2(
        stream_config_context: Rc<StreamConfigContext>,
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
            stream_config_context.tmp_file.download_tmp_file_size,
            stream_config_context.rate.download_limit_rate_after,
            stream_config_context.rate.download_limit_rate,
        );
        let upload_limit = StreamLimit::new(
            stream_config_context.tmp_file.upload_tmp_file_size,
            stream_config_context.rate.upload_limit_rate_after,
            stream_config_context.rate.upload_limit_rate,
        );

        let ret: Result<()> = async {
            tokio::select! {
                ret = StreamStream::do_single_stream_to_stream(
                    stream_config_context.clone(),
                    stream_info.clone(),
                    #[cfg(unix)] upstream_sendfile,
                    thread_id.clone(),
                    tmp_file_id.clone(),
                    upload_limit,
                    client_read,
                    upstream_write,
                    true,
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                ret =  StreamStream::do_single_stream_to_stream(
                    stream_config_context,
                    stream_info,
                    #[cfg(unix)] client_sendfile,
                    thread_id,
                    tmp_file_id,
                    download_limit,
                    upstream_read,
                    client_write,
                    false,
                ) => {
                    return ret.map_err(|e| anyhow!("err:ups => e:{}", e));
                }
                else => {
                    return Err(anyhow!("err:stream_to_stream_or_file select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn do_single_stream_to_stream(
        stream_config_context: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        #[cfg(unix)] sendfile: Option<SendFile>,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
        limit: StreamLimit,
        read: StreamFlowRead,
        write: StreamFlowWrite,
        is_client: bool,
    ) -> Result<()> {
        let ret: Result<()> = async {
            tokio::select! {
                _ = StreamStream::limit_timeout(limit.clone()) => {
                    Ok(())
                }
                ret = StreamStream::do_stream_to_stream(
                    limit,
                    stream_config_context.clone(),
                    stream_info.clone(),
                    read,
                    write,
                    #[cfg(unix)] sendfile,
                    is_client,
                    thread_id.clone(),
                    tmp_file_id.clone(),
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                else => {
                    return Err(anyhow!("err:stream_to_stream_or_file select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn limit_timeout(limit: StreamLimit) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            limit
                .curr_limit_rate
                .store(limit.max_limit_rate as i64, Ordering::Relaxed);
        }
    }

    pub async fn do_stream_to_stream<
        R: AsyncRead + AsyncReadMsg + AsyncStream + Unpin,
        W: AsyncWrite + AsyncWriteMsg + Unpin,
    >(
        limit: StreamLimit,
        stream_config_context: Rc<StreamConfigContext>,
        stream_info: Rc<RefCell<StreamInfo>>,
        mut r: R,
        w: W,
        #[cfg(unix)] sendfile: Option<SendFile>,
        is_client: bool,
        thread_id: std::thread::ThreadId,
        tmp_file_id: Rc<RefCell<u64>>,
    ) -> Result<()> {
        if limit.tmp_file_size > 0
            || stream_config_context.stream.stream_cache_size > 0
            || r.is_single().await
        {
            let mut cache_or_file = StreamStreamCacheOrFile::new(
                stream_info,
                r,
                w,
                #[cfg(unix)]
                sendfile,
                is_client,
                stream_config_context.stream.stream_cache_size,
            )?;
            cache_or_file
                .start(limit, stream_config_context, thread_id, tmp_file_id)
                .await
        } else {
            let mut memory = StreamStreamMemory::new();
            memory.start(limit, stream_info, r, w, is_client).await
        }
    }

    pub async fn stream_limit_write<W: AsyncWrite + AsyncWriteMsg + Unpin>(
        buffer: &mut StreamCacheBuffer,
        ssc: &mut StreamStreamContext,
        w: &mut W,
        is_client: bool,
        #[cfg(unix)] sendfile: &Option<SendFile>,
        stream_info: Rc<RefCell<StreamInfo>>,
    ) -> std::io::Result<StreamStatus> {
        let mut _is_sendfile = false;
        let mut n = buffer.size - buffer.start;
        let curr_limit_rate = ssc.curr_limit_rate.load(Ordering::Relaxed);
        let limit_rate_size = if ssc.max_limit_rate <= 0 {
            n
        } else if ssc.limit_rate_after > 0 {
            ssc.limit_rate_after as u64
        } else if curr_limit_rate > 0 {
            curr_limit_rate as u64
        } else {
            return Ok(StreamStatus::Limit);
        };

        if limit_rate_size < n {
            n = limit_rate_size;
        }
        log::trace!("{}, n:{}", StreamStream::get_flag(is_client), n);

        let wn = loop {
            #[cfg(unix)]
            if sendfile.is_some() && buffer.file_fd > 0 {
                if stream_info.borrow().debug_is_open_print {
                    let stream_info = stream_info.borrow();
                    log::info!(
                        "{}---{:?}---{}:sendfile stream_cache_size:{}, max_tmp_file_size:{}, tmp_file_size:{}, file_fd:{}, seek:{}",
                        stream_info.request_id,
                        stream_info.server_stream_info.local_addr,
                        StreamStream::get_flag(is_client),
                        ssc.stream_cache_size,
                        ssc.max_tmp_file_size,
                        ssc.tmp_file_size,
                        buffer.file_fd,
                        buffer.seek,
                    );
                }

                _is_sendfile = true;
                let wn = sendfile
                    .as_ref()
                    .unwrap()
                    .write(buffer.file_fd, buffer.seek, n)
                    .await;
                if let Err(e) = wn {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        return Ok(StreamStatus::StreamFull(StreamFullInfo::new(
                            _is_sendfile,
                            0,
                        )));
                    }
                    return Err(e);
                }
                break wn.unwrap();
            }

            _is_sendfile = false;
            let end = buffer.start + n;

            if stream_info.borrow().debug_is_open_print {
                let stream_info = stream_info.borrow();
                log::info!(
                    "{}---{:?}---{}:write stream_cache_size:{}, max_tmp_file_size:{}, tmp_file_size:{}, start:{}, end:{}",
                    stream_info.request_id,
                    stream_info.server_stream_info.local_addr,
                    StreamStream::get_flag(is_client),
                    ssc.stream_cache_size,
                    ssc.max_tmp_file_size,
                    ssc.tmp_file_size,
                    buffer.start,
                    end,
                );
            }

            log::trace!(
                "write:{:?}",
                buffer.data(buffer.start as usize, end as usize)
            );
            #[cfg(feature = "anyproxy-write-block-time-ms")]
            let write_start_time = Instant::now();

            let write_size = if w.is_write_msg().await {
                let msg = buffer.msg(buffer.start as usize, end as usize);
                w.write_msg(msg).await? as u64
            } else {
                w.write(buffer.data(buffer.start as usize, end as usize))
                    .await? as u64
            };

            #[cfg(feature = "anyproxy-write-block-time-ms")]
            {
                let write_max_block_time_ms = write_start_time.elapsed().as_millis();
                if write_max_block_time_ms > stream_info.borrow().write_max_block_time_ms {
                    stream_info.borrow_mut().write_max_block_time_ms = write_max_block_time_ms;
                }
            }
            break write_size;
        };

        ssc.total_write_size += wn;
        stream_info.borrow_mut().total_write_size += wn;
        log::trace!("{}, wn:{}", StreamStream::get_flag(is_client), wn);
        if ssc.max_limit_rate <= 0 {
            //
        } else if ssc.limit_rate_after > 0 {
            ssc.limit_rate_after -= wn as i64;
        } else {
            ssc.curr_limit_rate.fetch_sub(wn as i64, Ordering::Relaxed);
        }
        buffer.start += wn;
        buffer.seek += wn;
        if buffer.is_cache {
            if ssc.max_stream_cache_size > 0 {
                ssc.stream_cache_size += wn as i64;
            }
        } else {
            if ssc.max_tmp_file_size > 0 {
                ssc.tmp_file_size += wn as i64;
            }
        }

        if wn != n {
            return Ok(StreamStatus::StreamFull(StreamFullInfo::new(
                _is_sendfile,
                wn,
            )));
        }
        return Ok(StreamStatus::Ok(wn));
    }
}
