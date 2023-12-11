#[cfg(unix)]
use super::sendfile::SendFile;
use super::stream_info::StreamInfo;
use super::StreamConfigContext;
use super::StreamStreamContext;
use crate::protopack;
use crate::proxy::{
    get_flag, util as proxy_util, StreamStatus, StreamStreamData, StreamStreamShare,
};
use any_base::stream_flow::{StreamFlow, StreamFlowRead, StreamFlowWrite};
use any_base::typ::{ArcMutexTokio, ArcRwLock, ArcUnsafeAny, Share, ShareRw, ValueOption};
use anyhow::anyhow;
use anyhow::Result;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
#[cfg(feature = "anyproxy-write-block-time-ms")]
use std::time::Instant;
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

        if hello.is_some() {
            stream_info.get_mut().protocol_hello_size = protopack::anyproxy::write_pack(
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

            stream_info
                .get_mut()
                .add_work_time(&format!("first cache write {}", get_flag(true)));
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
        let ebpf_add_sock_hash = any_ebpf_core_conf.ebpf();
        if ebpf_add_sock_hash.is_none() {
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

    pub async fn ebpf_and_stream(
        scc: ShareRw<StreamConfigContext>,
        stream_info: Share<StreamInfo>,
        #[cfg(feature = "anyproxy-ebpf")] mut client_stream: StreamFlow,
        #[cfg(not(feature = "anyproxy-ebpf"))] client_stream: StreamFlow,
        #[cfg(feature = "anyproxy-ebpf")] mut upstream_stream: StreamFlow,
        #[cfg(not(feature = "anyproxy-ebpf"))] upstream_stream: StreamFlow,
    ) -> Result<()> {
        #[cfg(feature = "anyproxy-ebpf")]
        let client_local_addr = stream_info
            .get()
            .server_stream_info
            .local_addr
            .clone()
            .unwrap();
        #[cfg(feature = "anyproxy-ebpf")]
        let client_remote_addr = stream_info.get().server_stream_info.remote_addr;

        #[cfg(feature = "anyproxy-ebpf")]
        let ups_remote_addr = stream_info
            .get()
            .upstream_connect_info
            .get()
            .remote_addr
            .clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let ups_local_addr = stream_info
            .get()
            .upstream_connect_info
            .get()
            .local_addr
            .clone();

        #[cfg(feature = "anyproxy-ebpf")]
        use crate::config::any_ebpf_core;
        #[cfg(feature = "anyproxy-ebpf")]
        let ms = scc.get().ms.clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let any_ebpf_core_conf = any_ebpf_core::main_conf(&ms).await;
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = any_ebpf_core_conf.ebpf();

        let mut _is_open_ebpf = false;
        #[cfg(feature = "anyproxy-ebpf")]
        if scc.get().http_core_conf().is_open_ebpf
            && ebpf_add_sock_hash.is_some()
            && client_stream.fd() > 0
            && upstream_stream.fd() > 0
        {
            _is_open_ebpf = true;
            stream_info.get_mut().is_discard_timeout = true;
            stream_info.get_mut().add_work_time("ebpf");

            stream_info.get_mut().is_open_ebpf = true;

            log::trace!(
                "ebpf client_stream.fd():{},  upstream_stream.fd():{}",
                client_stream.fd(),
                upstream_stream.fd()
            );
            log::trace!(
                "ebpf client_remote_addr.fd():{},  client_local_addr.fd():{}",
                client_remote_addr,
                client_local_addr
            );

            log::trace!(
                "ebpf ups_remote_addr.fd():{},  ups_local_addr.fd():{}",
                ups_remote_addr,
                ups_local_addr
            );

            let rx = ebpf_add_sock_hash
                .as_ref()
                .unwrap()
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
        stream_info.get_mut().add_work_time("stream_to_stream");

        let _client_stream_fd = client_stream.fd();
        let _upstream_stream_fd = upstream_stream.fd();

        #[cfg(unix)]
        let (client_sendfile, upstream_sendfile) = {
            let scc = scc.get();
            let http_core_conf = scc.http_core_conf();
            if http_core_conf.is_open_sendfile && !_is_open_ebpf {
                let client_sendfile =
                    if _client_stream_fd > 0 && http_core_conf.upload_tmp_file_size > 0 {
                        stream_info.get_mut().open_sendfile = Some("sendfile_upload".to_string());
                        ArcMutexTokio::new(SendFile::new(
                            _client_stream_fd,
                            stream_info.get().client_stream_flow_info.clone(),
                        ))
                    } else {
                        ArcMutexTokio::default()
                    };

                let upstream_sendfile = if _upstream_stream_fd > 0
                    && http_core_conf.download_tmp_file_size > 0
                {
                    if stream_info.get_mut().open_sendfile.is_none() {
                        stream_info.get_mut().open_sendfile = Some("sendfile_upload".to_string());
                    } else {
                        let open_sendfile = stream_info.get_mut().open_sendfile.take().unwrap();
                        stream_info.get_mut().open_sendfile = Some(open_sendfile + "_download");
                    }
                    ArcMutexTokio::new(SendFile::new(
                        _upstream_stream_fd,
                        stream_info.get().upstream_stream_flow_info.clone(),
                    ))
                } else {
                    ArcMutexTokio::default()
                };
                (client_sendfile, upstream_sendfile)
            } else {
                (ArcMutexTokio::default(), ArcMutexTokio::default())
            }
        };

        let ret = StreamStream::stream_to_stream(
            scc.clone(),
            stream_info.clone(),
            client_stream,
            upstream_stream,
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

        #[cfg(feature = "anyproxy-ebpf")]
        if _is_open_ebpf && _client_stream_fd > 0 && _upstream_stream_fd > 0 {
            let rx = ebpf_add_sock_hash
                .as_ref()
                .unwrap()
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

    pub async fn limit_timeout_reset(ssc: StreamStreamContext) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            ssc.curr_limit_rate
                .store(ssc.cs.max_limit_rate as i64, Ordering::Relaxed);
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
            stream_status: StreamStatus::DataEmpty,
            caches: LinkedList::new(),
            buffer_pool: ValueOption::default(),
            write_err: None,
            plugins,
            is_first_write: true,
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
        (plugin_handle_stream)(sss).await?;
        Ok(())
    }
}
