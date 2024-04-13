use super::stream_info::StreamInfo;
use super::StreamConfigContext;
use super::StreamStreamContext;
use crate::protopack;
use crate::proxy::stream_stream_buf::StreamStreamCacheWriteDeque;
use crate::proxy::util::find_local;
use crate::proxy::{
    util as proxy_util, DelayState, StreamCloseType, StreamStreamData, StreamStreamShare,
    StreamStreamShareContext,
};
use any_base::future_wait::FutureWait;
use any_base::io::async_stream::Stream;
use any_base::io::async_write_msg::AsyncWriteMsg;
use any_base::stream_flow::{StreamFlow, StreamFlowRead, StreamFlowWrite};
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcUnsafeAny, Share, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

pub struct StreamStream {}

impl StreamStream {
    pub async fn connect_and_stream(
        stream_info: Share<StreamInfo>,
        client_buffer: &[u8],
        client_stream: StreamFlow,
    ) -> Result<()> {
        find_local(stream_info.clone())?;
        let scc = stream_info.get().scc.clone().unwrap();

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
            stream_info.get_mut().upstream_protocol_hello_size = hello_datas.len();
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

                stream_info
                    .get_mut()
                    .add_work_time1(&format!("header client to upstream: len={}", len));
            }
        }

        stream_info.get_mut().add_work_time1("ebpf_and_stream");
        Self::ebpf_and_stream(scc, stream_info, client_stream, upstream_stream).await
    }

    #[cfg(feature = "anyproxy-ebpf")]
    pub async fn connect_and_ebpf(
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
    ) -> Result<Option<StreamFlow>> {
        use crate::config::any_ebpf_core;
        use crate::Protocol7;
        let any_ebpf_core_conf = any_ebpf_core::main_conf(scc.ms()).await;
        let ebpf_tx = any_ebpf_core_conf.ebpf();
        if ebpf_tx.is_none() {
            return Ok(Some(client_stream));
        }

        find_local(stream_info.clone())?;
        let scc = stream_info.get().scc.clone().unwrap();

        let (_, connect_func) =
            proxy_util::upsteam_connect_info(stream_info.clone(), scc.clone()).await?;
        if connect_func.protocol7().await != Protocol7::Tcp.to_string() {
            return Ok(Some(client_stream));
        }

        use crate::proxy::util::run_plugin_handle_access;
        if run_plugin_handle_access(scc.clone(), stream_info.clone()).await? {
            return Ok(None);
        }

        let upstream_stream =
            proxy_util::upsteam_do_connect(stream_info.clone(), connect_func).await?;

        use crate::proxy::stream_info::ErrStatus;
        stream_info.get_mut().err_status = ErrStatus::Ok;
        stream_info.get_mut().add_work_time1("ebpf_and_stream");
        match Self::ebpf_and_stream(scc, stream_info, client_stream, upstream_stream).await {
            Ok(()) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn ebpf_stream(
        is_client: bool,
        r: &mut StreamFlowRead,
        w: &mut StreamFlowWrite,
        r_wait: FutureWait,
        w_wait: FutureWait,
        wait_group: &awaitgroup::WaitGroup,
        worker_inner: &awaitgroup::WorkerInner,
        scc: Arc<StreamConfigContext>,
    ) -> Result<()> {
        let mut data = vec![0; 8192];
        let data = data.as_mut_slice();
        let mut timeout = tokio::time::Duration::from_millis(u64::max_value());
        let wait_timeout = {
            let net_core_conf = scc.net_core_conf();
            let client_timeout_mil_time_ebpf = net_core_conf.client_timeout_mil_time_ebpf;
            let upstream_timeout_mil_time_ebpf = net_core_conf.upstream_timeout_mil_time_ebpf;
            if is_client {
                tokio::time::Duration::from_millis(client_timeout_mil_time_ebpf)
            } else {
                tokio::time::Duration::from_millis(upstream_timeout_mil_time_ebpf)
            }
        };
        let is_waker = loop {
            let r_wait = r_wait.clone();
            let n = tokio::select! {
                biased;
                n = r.read(data) => {
                    n
                },
                _ = r_wait => {
                    timeout = wait_timeout;
                    continue;
                },
                _= tokio::time::sleep(timeout) => {
                    break 0;
                },
                else => {
                    break 1;
                }
            };
            if n.is_err() {
                break 2;
            }
            let n = n.unwrap();
            if n == 0 {
                break 3;
            }

            let ret = w.write_all(&data[0..n]).await;
            if ret.is_err() {
                break 4;
            }
        };

        if is_waker > 0 {
            w_wait.waker();
        }

        worker_inner.done();
        let _ = wait_group.wait().await;

        Ok(())
    }

    pub async fn wait_ebpf_stream(
        scc: Arc<StreamConfigContext>,
        _stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
        upstream_stream: StreamFlow,
    ) -> Result<()> {
        let (mut client_read, mut client_write) = client_stream.split();
        let (mut upstream_read, mut upstream_write) = upstream_stream.split();

        let client_wait = FutureWait::new();
        let ups_wait = FutureWait::new();

        let wait_group = awaitgroup::WaitGroup::new();
        let client_worker_inner = wait_group.worker().add();
        let ups_worker_inner = wait_group.worker().add();

        tokio::select! {
            ret = Self::ebpf_stream(
                true,
                &mut client_read,
                &mut upstream_write,
                    client_wait.clone(),
                    ups_wait.clone(),
                &wait_group,
                &client_worker_inner,
                scc.clone(),
            )  => {
                return ret;
            }
            ret = Self::ebpf_stream(
                false,
                &mut upstream_read,
                &mut client_write,
                    ups_wait,
                    client_wait,
                &wait_group,
                &ups_worker_inner,
                scc,
            ) => {
                return ret;
            }
            else => {
                return Err(anyhow!("err:tokio::select!"));
            }
        }
    }

    pub fn is_sendfile(
        client_stream_fd: i32,
        upstream_stream_fd: i32,
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
    ) -> (bool, bool) {
        let mut stream_info = stream_info.get_mut();
        let net_core_conf = scc.net_core_conf();
        if net_core_conf.is_open_sendfile {
            let is_client_sendfile = if client_stream_fd > 0 {
                stream_info.open_sendfile = Some("sendfile_upload".to_string());
                true
            } else {
                false
            };

            let is_upstream_sendfile = if upstream_stream_fd > 0 {
                if stream_info.open_sendfile.is_none() {
                    stream_info.open_sendfile = Some("sendfile_upload".to_string());
                } else {
                    let open_sendfile = stream_info.open_sendfile.take().unwrap();
                    stream_info.open_sendfile = Some(open_sendfile + "_download");
                }
                true
            } else {
                false
            };
            (is_client_sendfile, is_upstream_sendfile)
        } else {
            (false, false)
        }
    }

    pub async fn ebpf_and_stream(
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
        mut _upstream_stream: StreamFlow,
    ) -> Result<()> {
        #[cfg(feature = "anyproxy-ebpf")]
        if scc.net_core_conf().is_open_ebpf {
            let client_stream_fd = client_stream.raw_fd();
            let upstream_stream_fd = _upstream_stream.raw_fd();

            use crate::config::any_ebpf_core;
            use crate::ebpf::any_ebpf::AnyEbpfTx;
            let any_ebpf_core_conf = any_ebpf_core::main_conf(scc.ms()).await;
            let ebpf_tx = any_ebpf_core_conf.ebpf();
            if ebpf_tx.is_some() && client_stream_fd > 0 && upstream_stream_fd > 0 {
                let sock_map_data = {
                    let mut stream_info = stream_info.get_mut();
                    let upstream_connect_info = stream_info.upstream_connect_info.clone();
                    let (remote_addr, local_addr) = {
                        let upstream_connect_info = upstream_connect_info.get();
                        (
                            upstream_connect_info.remote_addr.clone(),
                            upstream_connect_info.local_addr.clone(),
                        )
                    };

                    stream_info.is_discard_timeout = true;
                    stream_info.add_work_time1("ebpf");
                    stream_info.is_open_ebpf = true;

                    AnyEbpfTx::sock_map_data(
                        client_stream_fd,
                        stream_info.server_stream_info.remote_addr,
                        stream_info.server_stream_info.local_addr.clone().unwrap(),
                        upstream_stream_fd,
                        remote_addr,
                        local_addr,
                    )
                };

                log::trace!("ebpf sock_map_data:{:?}", sock_map_data);

                let rx = ebpf_tx
                    .as_ref()
                    .unwrap()
                    .add_sock_map_data_ext(sock_map_data.clone())
                    .await?;
                let _ = rx.recv().await;

                stream_info.get_mut().add_work_time1("wait_ebpf_stream");

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

                stream_info.get_mut().add_work_time1("stream_to_stream end");

                let rx = ebpf_tx
                    .as_ref()
                    .unwrap()
                    .del_sock_map_data_ext(sock_map_data)
                    .await?;
                let _ = rx.recv().await;
                return ret;
            }
        }

        let client_stream_fd = client_stream.raw_fd();
        let upstream_stream_fd = _upstream_stream.raw_fd();
        let (is_client_sendfile, is_upstream_sendfile) = Self::is_sendfile(
            client_stream_fd,
            upstream_stream_fd,
            scc.clone(),
            stream_info.clone(),
        );

        stream_info.get_mut().add_work_time1("stream_to_stream");
        let close_type = scc.net_core_conf().close_type;
        let ret = StreamStream::stream_to_stream(
            scc.clone(),
            stream_info.clone(),
            client_stream,
            _upstream_stream,
            is_client_sendfile,
            is_upstream_sendfile,
            close_type,
            close_type,
        )
        .await
        .map_err(|e| {
            anyhow!(
                "err:stream_to_stream => request_id:{}, e:{}",
                stream_info.get().request_id,
                e
            )
        });

        stream_info.get_mut().add_work_time1("stream_to_stream end");

        ret
    }

    pub async fn stream_to_stream(
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client: StreamFlow,
        upstream: StreamFlow,
        is_client_sendfile: bool,
        is_upstream_sendfile: bool,
        client_close_type: StreamCloseType,
        upstream_close_type: StreamCloseType,
    ) -> Result<()> {
        let wait_group = awaitgroup::WaitGroup::new();
        let client_worker_inner = wait_group.worker().add();
        let ups_worker_inner = wait_group.worker().add();

        let upload_read = FutureWait::new();
        let download_read = FutureWait::new();
        let upload_close = FutureWait::new();
        let download_close = FutureWait::new();

        let (client_read, client_write) = client.split();
        let (upstream_read, upstream_write) = upstream.split();
        let download = scc.net_core_conf().download.clone();
        let ssc_download = Arc::new(StreamStreamContext {
            cs: download.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(download.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: download.stream_cache_size,
                tmp_file_size: download.tmp_file_size,
                tmp_file_curr_reopen_size: 0,
                limit_rate_after: download.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
                stream_nodelay_size: download.stream_nodelay_size,
            }),
            delay_version: AtomicUsize::new(0),
            delay_state: ArcRwLock::new(DelayState::Init),
            delay_wait: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_sendfile: is_client_sendfile,
            is_client: false,
            close_type: upstream_close_type,
            wait_group: wait_group.clone(),
            worker_inner: ups_worker_inner,
            read_wait: download_read.clone(),
            other_read_wait: upload_read.clone(),
            close_wait: download_close.clone(),
            other_close_wait: upload_close.clone(),
        });

        let upload = scc.net_core_conf().upload.clone();
        let ssc_upload = Arc::new(StreamStreamContext {
            cs: upload.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(upload.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: upload.stream_cache_size,
                tmp_file_size: upload.tmp_file_size,
                tmp_file_curr_reopen_size: 0,
                limit_rate_after: upload.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
                stream_nodelay_size: upload.stream_nodelay_size,
            }),
            delay_version: AtomicUsize::new(0),
            delay_state: ArcRwLock::new(DelayState::Init),
            delay_wait: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_sendfile: is_upstream_sendfile,
            is_client: true,
            close_type: client_close_type,
            wait_group,
            worker_inner: client_worker_inner,
            read_wait: upload_read,
            other_read_wait: download_read,
            close_wait: upload_close,
            other_close_wait: download_close,
        });

        {
            let mut stream_info = stream_info.get_mut();
            stream_info.ssc_download.set(ssc_download.clone());
            stream_info.ssc_upload.set(ssc_upload.clone());
        }

        stream_info
            .get_mut()
            .add_work_time1("do_single_stream_to_stream");
        let ret: Result<()> = async {
            tokio::select! {
                ret = StreamStream::do_single_stream_to_stream(
                    ssc_upload,
                    scc.clone(),
                    stream_info.clone(),
                    client_read,
                    upstream_write,
                )  => {
                    return ret.map_err(|e| anyhow!("err:client => e:{}", e));
                }
                ret =  StreamStream::do_single_stream_to_stream(
                    ssc_download,
                    scc,
                    stream_info,
                    upstream_read,
                    client_write,
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

    pub async fn stream_single(
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client: StreamFlow,
        is_sendfile: bool,
        is_download: bool,
    ) -> Result<()> {
        let wait_group = awaitgroup::WaitGroup::new();
        let client_worker_inner = wait_group.worker().add();

        let upload_read = FutureWait::new();
        let download_read = FutureWait::new();
        let upload_close = FutureWait::new();
        let download_close = FutureWait::new();

        let (client_read, client_write) = client.split();
        let download = if is_download {
            scc.net_core_conf().download.clone()
        } else {
            scc.net_core_conf().upload.clone()
        };
        let ssc_download = Arc::new(StreamStreamContext {
            cs: download.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(download.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: download.stream_cache_size,
                tmp_file_size: download.tmp_file_size,
                tmp_file_curr_reopen_size: 0,
                limit_rate_after: download.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
                stream_nodelay_size: download.stream_nodelay_size,
            }),
            delay_version: AtomicUsize::new(0),
            delay_state: ArcRwLock::new(DelayState::Init),
            delay_wait: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_sendfile,
            is_client: false,
            close_type: StreamCloseType::Fast,
            wait_group: wait_group.clone(),
            worker_inner: client_worker_inner,
            read_wait: download_read.clone(),
            other_read_wait: upload_read.clone(),
            close_wait: download_close.clone(),
            other_close_wait: upload_close.clone(),
        });

        {
            let mut stream_info = stream_info.get_mut();
            if is_download {
                stream_info.ssc_download.set(ssc_download.clone());
            } else {
                stream_info.ssc_upload.set(ssc_download.clone());
            }
        }

        stream_info
            .get_mut()
            .add_work_time1("do_single_stream_to_stream");

        StreamStream::do_single_stream_to_stream(
            ssc_download,
            scc,
            stream_info,
            client_read,
            client_write,
        )
        .await
        .map_err(|e| anyhow!("err:client => e:{}", e))
    }

    pub async fn stream_to_stream_single(
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        client: StreamFlow,
        upstream: StreamFlow,
        is_client_sendfile: bool,
        is_upstream_sendfile: bool,
    ) -> Result<()> {
        let wait_group = awaitgroup::WaitGroup::new();
        let client_worker_inner = wait_group.worker().add();
        let ups_worker_inner = wait_group.worker().add();

        let upload_read = FutureWait::new();
        let download_read = FutureWait::new();
        let upload_close = FutureWait::new();
        let download_close = FutureWait::new();

        let (client_read, client_write) = client.split();
        let (upstream_read, upstream_write) = upstream.split();
        let download = scc.net_core_conf().download.clone();
        let ssc_download = Arc::new(StreamStreamContext {
            cs: download.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(download.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: download.stream_cache_size,
                tmp_file_size: download.tmp_file_size,
                tmp_file_curr_reopen_size: 0,
                limit_rate_after: download.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
                stream_nodelay_size: download.stream_nodelay_size,
            }),
            delay_version: AtomicUsize::new(0),
            delay_state: ArcRwLock::new(DelayState::Init),
            delay_wait: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_sendfile: is_client_sendfile,
            is_client: false,
            close_type: StreamCloseType::Fast,
            wait_group: wait_group.clone(),
            worker_inner: ups_worker_inner,
            read_wait: download_read.clone(),
            other_read_wait: upload_read.clone(),
            close_wait: download_close.clone(),
            other_close_wait: upload_close.clone(),
        });

        let upload = scc.net_core_conf().upload.clone();
        let ssc_upload = Arc::new(StreamStreamContext {
            cs: upload.clone(),
            curr_limit_rate: Arc::new(AtomicI64::new(upload.max_limit_rate as i64)),
            ssd: ArcRwLock::new(StreamStreamData {
                stream_cache_size: upload.stream_cache_size,
                tmp_file_size: upload.tmp_file_size,
                tmp_file_curr_reopen_size: 0,
                limit_rate_after: upload.limit_rate_after,
                total_read_size: 0,
                total_write_size: 0,
                stream_nodelay_size: upload.stream_nodelay_size,
            }),
            delay_version: AtomicUsize::new(0),
            delay_state: ArcRwLock::new(DelayState::Init),
            delay_wait: FutureWait::new(),
            delay_ok: Arc::new(AtomicBool::new(false)),
            is_sendfile: is_upstream_sendfile,
            is_client: true,
            close_type: StreamCloseType::Fast,
            wait_group,
            worker_inner: client_worker_inner,
            read_wait: upload_read,
            other_read_wait: download_read,
            close_wait: upload_close,
            other_close_wait: download_close,
        });

        {
            let mut stream_info = stream_info.get_mut();
            stream_info.ssc_download.set(ssc_download.clone());
            stream_info.ssc_upload.set(ssc_upload.clone());
        }

        stream_info
            .get_mut()
            .add_work_time1("do_single_stream_to_stream1");
        let ret = StreamStream::do_single_stream_to_stream(
            ssc_upload,
            scc.clone(),
            stream_info.clone(),
            client_read,
            upstream_write,
        )
        .await
        .map_err(|e| anyhow!("err:client => e:{}", e));

        if let Err(e) = ret {
            if !stream_info.get().client_stream_flow_info.get().is_close() {
                return Err(e);
            }
        }

        stream_info
            .get_mut()
            .add_work_time1("do_single_stream_to_stream2");
        StreamStream::do_single_stream_to_stream(
            ssc_download,
            scc,
            stream_info,
            upstream_read,
            client_write,
        )
        .await
        .map_err(|e| anyhow!("err:ups => e:{}", e))
    }

    pub async fn do_single_stream_to_stream(
        ssc: Arc<StreamStreamContext>,
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        read: StreamFlowRead,
        write: StreamFlowWrite,
    ) -> Result<()> {
        stream_info
            .get_mut()
            .add_work_time2(ssc.is_client, "do_stream_to_stream");
        let ret: Result<()> = async {
            tokio::select! {
                _ = StreamStream::limit_timeout_reset(ssc.clone()) => {
                    Ok(())
                }
                _ = StreamStream::delay_timeout_reset(ssc.clone(), stream_info.clone()) => {
                    Ok(())
                }
                ret = StreamStream::do_stream_to_stream(
                    ssc,
                    scc.clone(),
                    stream_info.clone(),
                    read,
                    write,
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

    pub async fn limit_timeout_reset(ssc: Arc<StreamStreamContext>) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            ssc.curr_limit_rate
                .store(ssc.cs.max_limit_rate as i64, Ordering::Relaxed);
        }
    }

    pub async fn delay_timeout_reset(
        ssc: Arc<StreamStreamContext>,
        _stream_info: Share<StreamInfo>,
    ) {
        let mut stream_delay_mil_time = ssc.cs.stream_delay_mil_time;
        if stream_delay_mil_time <= 0 {
            stream_delay_mil_time = u64::MAX;
            tokio::time::sleep(tokio::time::Duration::from_millis(stream_delay_mil_time)).await;
        }
        *ssc.delay_state.get_mut() = DelayState::Wait;
        loop {
            if *ssc.delay_state.get() == DelayState::Wait {
                ssc.delay_wait.clone().await;
            }

            let version = ssc.delay_version.load(Ordering::SeqCst);

            ssc.delay_ok.store(false, Ordering::SeqCst);

            *ssc.delay_state.get_mut() = DelayState::Start;
            tokio::select! {
                biased;
                _= tokio::time::sleep(tokio::time::Duration::from_millis(stream_delay_mil_time)) => {
                    let version2 = ssc.delay_version.load(Ordering::SeqCst);
                    if version == version2 && *ssc.delay_state.get() == DelayState::Start {
                        *ssc.delay_state.get_mut() = DelayState::Wait;
                        ssc.delay_ok.store(true, Ordering::SeqCst);
                        ssc.read_wait.waker();
                    }
                },
                else => {
                    continue;
                }
            }
        }
    }

    pub async fn do_stream_to_stream(
        ssc: Arc<StreamStreamContext>,
        scc: Arc<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        r: StreamFlowRead,
        w: StreamFlowWrite,
    ) -> Result<()> {
        let (_w_raw_fd, is_write_msg, is_write_vectored) =
            { (w.raw_fd(), w.is_write_msg(), w.is_write_vectored()) };

        let plugin_handle_stream = ssc.cs.plugin_handle_stream.clone();
        use crate::config::net_core;
        let ctx_index_len = net_core::module().get().ctx_index_len as usize;
        let plugins: Vec<Option<ArcUnsafeAny>> = vec![None; ctx_index_len];
        let session_id = stream_info.get().session_id;
        use crate::util::default_config::PAGE_SIZE;
        let page_size = PAGE_SIZE.load(Ordering::Relaxed);
        let sss = Arc::new(StreamStreamShare {
            sss_ctx: ShareRw::new(StreamStreamShareContext {
                write_buffer_deque: Some(StreamStreamCacheWriteDeque::new(
                    ssc.cs.write_buffer_size,
                )),
                write_buffer: None,
                read_buffer: None,
                read_err: None,
                caches: LinkedList::new(),
                write_err: None,
                plugins,
                is_first_write: true,
                is_stream_cache: false,
                #[cfg(unix)]
                is_set_tcp_nopush: false,
            }),
            ssc: ssc.clone(),
            _scc: scc.clone(),
            stream_info: stream_info.clone(),
            r: ArcMutexTokio::new(r),
            w: ArcMutexTokio::new(w),
            _w_raw_fd,
            is_write_msg,
            is_write_vectored,
            _session_id: session_id,
            page_size,
        });

        let plugin_handle_stream = if let &StreamCloseType::WaitEmpty = &ssc.close_type {
            stream_info
                .get_mut()
                .add_work_time2(ssc.is_client, "WaitEmpty");
            let ms = scc.ms.clone();
            use crate::config::net_core_plugin;
            let net_core_plugin_conf = net_core_plugin::main_conf(&ms).await;
            net_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            stream_info
                .get_mut()
                .add_work_time2(ssc.is_client, "!= WaitEmpty");
            plugin_handle_stream
        };

        let plugin_handle_stream = plugin_handle_stream.get().await;
        let _ = (plugin_handle_stream)(sss).await?;
        Ok(())
    }
}
