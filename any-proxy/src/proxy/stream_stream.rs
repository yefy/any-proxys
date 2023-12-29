#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info::StreamInfo;
use super::StreamConfigContext;
use super::StreamStreamContext;
use crate::protopack;
use crate::proxy::{get_flag, util as proxy_util, StreamStreamData, StreamStreamShare};
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::future_wait::FutureWait;
use any_base::stream_flow::{StreamFlow, StreamFlowRead, StreamFlowWrite};
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcUnsafeAny, Share, ShareRw, ValueOption};
use anyhow::anyhow;
use anyhow::Result;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub struct StreamStream {}

impl StreamStream {
    pub async fn connect_and_stream(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_buffer: &[u8],
        client_stream: StreamFlow,
    ) -> Result<()> {
        let (is_proxy_protocol_hello, mut upstream_stream) =
            proxy_util::upsteam_connect(stream_info.clone(), scc.clone()).await?;

        let hello =
            proxy_util::get_proxy_hello(is_proxy_protocol_hello, stream_info.clone(), scc.clone())
                .await;

        // if hello.is_some() {
        //     stream_info.get_mut().protocol_hello_size = protopack::anyproxy::write_pack(
        //         &mut upstream_stream,
        //         protopack::anyproxy::AnyproxyHeaderType::Hello,
        //         &*hello.unwrap(),
        //     )
        //     .await
        //     .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
        // }

        let mut datas = if hello.is_some() {
            let hello_datas = protopack::anyproxy::pack_to_vec(
                protopack::anyproxy::AnyproxyHeaderType::Hello,
                &*hello.unwrap(),
            )
            .await
            .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;
            log::trace!("hello len:{:?}", hello_datas.len());
            Some(hello_datas)
        } else {
            None
        };

        let client_buffer = if client_buffer.len() > 0 {
            let len = client_buffer.len();
            log::trace!("first cache len:{}, data:{:?}", len, &client_buffer[0..len]);
            if datas.is_some() {
                let datas = datas.as_mut().unwrap();
                datas.extend_from_slice(&client_buffer[0..len]);
                Some(datas.as_slice())
            } else {
                Some(&client_buffer[0..len])
            }
        } else {
            if datas.is_some() {
                let datas = datas.as_mut().unwrap();
                Some(datas.as_slice())
            } else {
                None
            }
        };

        if client_buffer.is_some() {
            let client_buffer = client_buffer.unwrap();
            if client_buffer.len() > 0 {
                let len = client_buffer.len();
                log::trace!("write:{:?}", &client_buffer[0..len]);
                upstream_stream
                    .write(&client_buffer[0..len])
                    .await
                    .map_err(|e| anyhow!("err:upstream_stream.write => e:{}", e))?;

                let _ = upstream_stream.flush().await;

                stream_info.get_mut().add_work_time(&format!(
                    "first cache write {}: len={}",
                    get_flag(true),
                    len
                ));
            }
        }

        Self::ebpf_and_stream(scc, stream_info, client_stream, upstream_stream).await
    }

    #[cfg(feature = "anyproxy-ebpf")]
    pub async fn connect_and_ebpf(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
    ) -> Result<Option<StreamFlow>> {
        use crate::config::any_ebpf_core;
        use crate::Protocol7;
        let ms = scc.get().ms.clone();
        let any_ebpf_core_conf = any_ebpf_core::main_conf(&ms).await;
        let ebpf_tx = any_ebpf_core_conf.ebpf();
        if ebpf_tx.is_none() {
            return Ok(Some(client_stream));
        }

        let (_, connect_func) =
            proxy_util::upsteam_connect_info(stream_info.clone(), scc.clone()).await?;
        if connect_func.protocol7().await != Protocol7::Tcp.to_string() {
            return Ok(Some(client_stream));
        }
        let upstream_stream =
            proxy_util::upsteam_do_connect(stream_info.clone(), connect_func).await?;

        use crate::proxy::stream_info::ErrStatus;
        stream_info.get_mut().err_status = ErrStatus::Ok;
        match Self::ebpf_and_stream(scc, stream_info, client_stream, upstream_stream).await {
            Ok(()) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn wait_ebpf_stream(
        _scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
        upstream_stream: StreamFlow,
    ) -> Result<()> {
        let data_len = 8192;
        let (mut client_read, mut client_write) = client_stream.split();
        let (mut upstream_read, mut upstream_write) = upstream_stream.split();
        let executors = stream_info.get().executors.clone().unwrap();
        let mut executor = ExecutorLocalSpawn::new(executors);
        executor._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
            move |_| async move {
                let mut data = Vec::with_capacity(data_len);
                unsafe { data.set_len(data_len) };
                let data = data.as_mut_slice();
                loop {
                    let n = client_read.read(data).await;
                    if n.is_err() {
                        break;
                    }
                    let n = n.unwrap();
                    if n == 0 {
                        break;
                    }

                    let n = upstream_write.write_all(&data[0..n]).await;
                    if n.is_err() {
                        break;
                    }
                }
                Ok(())
            },
        );

        let mut data = Vec::with_capacity(data_len);
        unsafe { data.set_len(data_len) };
        let data = data.as_mut_slice();
        loop {
            let n = upstream_read.read(data).await;
            if n.is_err() {
                break;
            }
            let n = n.unwrap();
            if n == 0 {
                break;
            }
            let n = client_write.write_all(&data[0..n]).await;
            if n.is_err() {
                break;
            }
        }
        let _ = executor.wait("ebpf").await;
        Ok(())
    }

    pub async fn ebpf_and_stream(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
        _upstream_stream: StreamFlow,
    ) -> Result<()> {
        #[cfg(feature = "anyproxy-ebpf")]
        if scc.get().http_core_conf().is_open_ebpf {
            use crate::config::any_ebpf_core;
            use crate::ebpf::any_ebpf::AnyEbpfTx;
            let ms = scc.get().ms.clone();
            let any_ebpf_core_conf = any_ebpf_core::main_conf(&ms).await;
            let ebpf_tx = any_ebpf_core_conf.ebpf();
            if ebpf_tx.is_some() && client_stream.fd() > 0 && _upstream_stream.fd() > 0 {
                let sock_map_data = {
                    let mut stream_info = stream_info.get_mut();
                    let upstream_connect_info = stream_info.upstream_connect_info.clone();
                    let upstream_connect_info = upstream_connect_info.get();

                    stream_info.is_discard_timeout = true;
                    stream_info.add_work_time("ebpf");
                    stream_info.is_open_ebpf = true;

                    AnyEbpfTx::sock_map_data(
                        client_stream.fd(),
                        stream_info.server_stream_info.remote_addr,
                        stream_info.server_stream_info.local_addr.clone().unwrap(),
                        _upstream_stream.fd(),
                        upstream_connect_info.remote_addr.clone(),
                        upstream_connect_info.local_addr.clone(),
                    )
                };

                log::trace!("ebpf sock_map_data:{:?}", sock_map_data);

                let rx = ebpf_tx
                    .as_ref()
                    .unwrap()
                    .add_sock_map_data_ext(sock_map_data.clone())
                    .await?;
                let _ = rx.recv().await;

                stream_info.get_mut().add_work_time("stream_to_stream");

                let ret = StreamStream::wait_ebpf_stream(
                    scc.clone(),
                    stream_info.clone(),
                    client_stream,
                    _upstream_stream,
                )
                .await
                .map_err(|e| {
                    anyhow!(
                        "err:stream_to_stream => request_id:{}, e:{}",
                        stream_info.get().request_id,
                        e
                    )
                });

                stream_info.get_mut().add_work_time("stream_to_stream end");

                let rx = ebpf_tx
                    .as_ref()
                    .unwrap()
                    .del_sock_map_data_ext(sock_map_data)
                    .await?;
                let _ = rx.recv().await;
                return ret;
            }
        }

        #[cfg(unix)]
        let (client_sendfile, upstream_sendfile) = {
            let mut stream_info = stream_info.get_mut();
            let scc = scc.get();
            let http_core_conf = scc.http_core_conf();
            if http_core_conf.is_open_sendfile {
                let client_stream_fd = client_stream.fd();
                let client_sendfile =
                    if client_stream_fd > 0 && http_core_conf.upload_tmp_file_size > 0 {
                        stream_info.open_sendfile = Some("sendfile_upload".to_string());
                        ArcMutexTokio::new(SendFile::new(
                            client_stream_fd,
                            stream_info.client_stream_flow_info.clone(),
                        ))
                    } else {
                        ArcMutexTokio::default()
                    };

                let upstream_stream_fd = _upstream_stream.fd();
                let upstream_sendfile =
                    if upstream_stream_fd > 0 && http_core_conf.download_tmp_file_size > 0 {
                        if stream_info.open_sendfile.is_none() {
                            stream_info.open_sendfile = Some("sendfile_upload".to_string());
                        } else {
                            let open_sendfile = stream_info.open_sendfile.take().unwrap();
                            stream_info.open_sendfile = Some(open_sendfile + "_download");
                        }
                        ArcMutexTokio::new(SendFile::new(
                            upstream_stream_fd,
                            stream_info.upstream_stream_flow_info.clone(),
                        ))
                    } else {
                        ArcMutexTokio::default()
                    };
                (client_sendfile, upstream_sendfile)
            } else {
                (ArcMutexTokio::default(), ArcMutexTokio::default())
            }
        };

        stream_info.get_mut().add_work_time("stream_to_stream");

        let ret = StreamStream::stream_to_stream(
            scc.clone(),
            stream_info.clone(),
            client_stream,
            _upstream_stream,
            #[cfg(unix)]
            client_sendfile,
            #[cfg(unix)]
            upstream_sendfile,
            true,
            true,
        )
        .await
        .map_err(|e| {
            anyhow!(
                "err:stream_to_stream => request_id:{}, e:{}",
                stream_info.get().request_id,
                e
            )
        });

        stream_info.get_mut().add_work_time("stream_to_stream end");

        ret
    }

    pub async fn stream_to_stream(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client: StreamFlow,
        upstream: StreamFlow,
        #[cfg(unix)] client_sendfile: ArcMutexTokio<SendFile>,
        #[cfg(unix)] upstream_sendfile: ArcMutexTokio<SendFile>,
        is_fast_close_client: bool,
        is_fast_close_upstream: bool,
    ) -> Result<()> {
        let (client_read, client_write) = client.split();
        let (upstream_read, upstream_write) = upstream.split();
        let download = scc.get().http_core_conf().download.clone();
        let ssc_download = StreamStreamContext {
            cs: download.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(download.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: download.stream_cache_size,
                tmp_file_size: download.tmp_file_size,
                limit_rate_after: download.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
            }),
            delay_wait: FutureWait::new(),
            delay_wait_stop: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_delay: Arc::new(AtomicBool::new(false)),
        };

        let upload = scc.get().http_core_conf().upload.clone();
        let ssc_upload = StreamStreamContext {
            cs: upload.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(upload.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: upload.stream_cache_size,
                tmp_file_size: upload.tmp_file_size,
                limit_rate_after: upload.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
            }),
            delay_wait: FutureWait::new(),
            delay_wait_stop: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_delay: Arc::new(AtomicBool::new(false)),
        };

        let ret: Result<()> = async {
            tokio::select! {
                ret = StreamStream::do_single_stream_to_stream(
                    ssc_upload,
                    scc.clone(),
                    stream_info.clone(),
                    client_read,
                    upstream_write,
                    #[cfg(unix)] upstream_sendfile,
                    true,
                    is_fast_close_client,
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                ret =  StreamStream::do_single_stream_to_stream(
                    ssc_download,
                    scc,
                    stream_info,
                    upstream_read,
                    client_write,
                    #[cfg(unix)] client_sendfile,
                    false,
                    is_fast_close_upstream,
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

    pub async fn stream_to_stream_single(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client: StreamFlow,
        upstream: StreamFlow,
        #[cfg(unix)] client_sendfile: ArcMutexTokio<SendFile>,
        #[cfg(unix)] upstream_sendfile: ArcMutexTokio<SendFile>,
    ) -> Result<()> {
        let (client_read, client_write) = client.split();
        let (upstream_read, upstream_write) = upstream.split();
        let download = scc.get().http_core_conf().download.clone();
        let ssc_download = StreamStreamContext {
            cs: download.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(download.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: download.stream_cache_size,
                tmp_file_size: download.tmp_file_size,
                limit_rate_after: download.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
            }),
            delay_wait: FutureWait::new(),
            delay_wait_stop: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_delay: Arc::new(AtomicBool::new(false)),
        };

        let upload = scc.get().http_core_conf().upload.clone();
        let ssc_upload = StreamStreamContext {
            cs: upload.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(upload.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: upload.stream_cache_size,
                tmp_file_size: upload.tmp_file_size,
                limit_rate_after: upload.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
            }),
            delay_wait: FutureWait::new(),
            delay_wait_stop: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_delay: Arc::new(AtomicBool::new(false)),
        };

        let is_fast_close = true;
        {
            let ret = StreamStream::do_single_stream_to_stream(
                ssc_upload,
                scc.clone(),
                stream_info.clone(),
                client_read,
                upstream_write,
                #[cfg(unix)]
                upstream_sendfile,
                true,
                is_fast_close,
            )
            .await
            .map_err(|e| anyhow!("err:client => e:{}", e));
            match ret {
                Err(e) => {
                    if !stream_info.get().client_stream_flow_info.get().is_close() {
                        return Err(e);
                    }
                }
                Ok(()) => {}
            }
        }

        StreamStream::do_single_stream_to_stream(
            ssc_download,
            scc,
            stream_info,
            upstream_read,
            client_write,
            #[cfg(unix)]
            client_sendfile,
            false,
            is_fast_close,
        )
        .await
        .map_err(|e| anyhow!("err:ups => e:{}", e))
    }

    pub async fn do_single_stream_to_stream(
        ssc: StreamStreamContext,
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        read: StreamFlowRead,
        write: StreamFlowWrite,
        #[cfg(unix)] sendfile: ArcMutexTokio<SendFile>,
        is_client: bool,
        is_fast_close: bool,
    ) -> Result<()> {
        let ret: Result<()> = async {
            tokio::select! {
                _ = StreamStream::limit_timeout_reset(ssc.clone()) => {
                    Ok(())
                }
                _ = StreamStream::delay_timeout_reset(is_client, ssc.clone(), stream_info.clone()) => {
                    Ok(())
                }
                ret = StreamStream::do_stream_to_stream(
                    ssc,
                    scc.clone(),
                    stream_info.clone(),
                    read,
                    write,
                    #[cfg(unix)] sendfile,
                    is_client,
                    is_fast_close,
                )  => {
                    return ret.map_err(|e| anyhow!("err:do_stream_to_stream => e:{}", e));
                }
                else => {
                    return Err(anyhow!("err:do_single_stream_to_stream select close"));
                }
            }
        }
        .await;
        ret
    }

    pub async fn limit_timeout_reset(ssc: StreamStreamContext) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            ssc.curr_limit_rate
                .store(ssc.cs.max_limit_rate as i64, Ordering::Relaxed);
        }
    }

    pub async fn delay_timeout_reset(
        is_client: bool,
        ssc: StreamStreamContext,
        stream_info: Share<StreamInfo>,
    ) {
        let open_stream_cache_merge_mil_time = ssc.cs.open_stream_cache_merge_mil_time;
        loop {
            ssc.delay_wait.clone().await;
            ssc.is_delay.store(true, Ordering::SeqCst);
            ssc.delay_ok.store(false, Ordering::SeqCst);

            tokio::select! {
                biased;
                _ = ssc.delay_wait_stop.clone() => {
                },
                _= tokio::time::sleep(tokio::time::Duration::from_millis(open_stream_cache_merge_mil_time)) => {
                    ssc.delay_ok.store(true, Ordering::SeqCst);
                    let read_wait = if is_client {
                        stream_info.get().upload_read.clone()
                    } else {
                        stream_info.get().download_read.clone()
                    };
                    read_wait.waker();
                },
                else => {
                    continue;
                }
            }
            ssc.is_delay.store(false, Ordering::SeqCst);
        }
    }

    pub async fn do_stream_to_stream(
        ssc: StreamStreamContext,
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        r: StreamFlowRead,
        w: StreamFlowWrite,
        #[cfg(unix)] sendfile: ArcMutexTokio<SendFile>,
        is_client: bool,
        is_fast_close: bool,
    ) -> Result<()> {
        let plugin_handle_stream = ssc.cs.plugin_handle_stream.clone();
        use crate::config::http_core;
        let ctx_index_len = http_core::module().get().ctx_index_len as usize;
        let plugins: Vec<Option<ArcUnsafeAny>> = vec![None; ctx_index_len];
        let sss = StreamStreamShare {
            ssc,
            _scc: scc.clone(),
            stream_info,
            r: ArcMutexTokio::new(r),
            w: ArcMutexTokio::new(w),
            #[cfg(unix)]
            sendfile,
            is_client,
            is_fast_close,
            write_buffer: None,
            read_buffer: None,
            read_buffer_ret: None,
            read_err: None,
            caches: LinkedList::new(),
            buffer_pool: ValueOption::default(),
            write_err: None,
            plugins,
            is_first_write: true,
            is_stream_cache: false,
        };
        let sss = ShareRw::new(sss);

        let plugin_handle_stream = if !is_fast_close {
            let ms = scc.get().ms.clone();
            use crate::config::http_core_plugin;
            let http_core_plugin_conf = http_core_plugin::main_conf(&ms).await;
            http_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            plugin_handle_stream
        };

        let plugin_handle_stream = plugin_handle_stream.get().await;
        let _ = (plugin_handle_stream)(sss).await?;
        Ok(())
    }
}
