use super::access_log::AccessLog;
use super::proxy;
use super::stream_info;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use super::StreamConfigContext;
use crate::proxy::StreamTimeout;
use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use any_base::stream_flow;
use any_base::typ::{ArcMutex, Share, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

pub async fn do_start(
    mut start_stream: impl proxy::Stream,
    stream_info: Share<StreamInfo>,
    stream: stream_flow::StreamFlow,
    mut shutdown_thread_rx: broadcast::Receiver<bool>,
    debug_print_access_log_time: u64,
    debug_print_stream_flow_time: u64,
    stream_so_singer_time: usize,
) -> Result<()> {
    let start_time = Instant::now();
    let stream_timeout = StreamTimeout::new();

    let client_stream_flow_info = stream_info.get().client_stream_flow_info.clone();
    let upstream_stream_flow_info = stream_info.get().upstream_stream_flow_info.clone();
    let ret: Option<Result<()>> = async {
        tokio::select! {
            biased;
            _ret = start_stream.do_start(stream_info.clone(), stream) => {
                let _ret = _ret.map_err(|e| anyhow!("err:do_start => request_id:{}, e:{}",
                    stream_info.get().request_id, e));
                return Some(_ret);
            }
            _ret = read_timeout(
                client_stream_flow_info.clone(),
                stream_info.clone(),
                stream_timeout.clone(),
                stream_timeout.client_read_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = read_timeout(
                upstream_stream_flow_info.clone(),
                stream_info.clone(),
                stream_timeout.clone(),
                stream_timeout.ups_read_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = write_timeout(
                client_stream_flow_info.clone(),
                stream_info.clone(),
                stream_timeout.clone(),
                stream_timeout.client_write_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = write_timeout(
                upstream_stream_flow_info.clone(),
                stream_info.clone(),
                stream_timeout.clone(),
                stream_timeout.ups_write_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = debug_print_access_log(debug_print_access_log_time, stream_info.clone()) => {
                return Some(_ret);
            }
             _ret = debug_print_stream_flow(debug_print_stream_flow_time, stream_info.clone()) => {
                return Some(_ret);
            }
            _ = shutdown_thread_rx.recv() => {
                stream_info.get_mut().err_status = ErrStatus::ServerErr;
                return None;
            }
            else => {
                stream_info.get_mut().err_status = ErrStatus::ServerErr;
                return None;
            }
        }
    }
    .await;

    if stream_info.get().debug_is_open_print {
        let stream_info = stream_info.get();
        log::info!(
            "{}---{:?}:do_start end",
            stream_info.request_id,
            stream_info.server_stream_info.local_addr,
        );
    }

    let session_time = start_time.elapsed().as_secs_f32();
    stream_info.get_mut().session_time = session_time;

    let (is_close, is_ups_close) = stream_info_parse(stream_timeout, stream_info.clone())
        .await
        .map_err(|e| anyhow!("err:stream_connect_parse => e:{}", e))?;

    if stream_info.get().scc.is_some() {
        let plugin_handle_logs = {
            let stream_info = stream_info.get();
            let scc = stream_info.scc.get();
            use crate::config::http_core_plugin;
            let http_core_plugin_conf = http_core_plugin::currs_conf(scc.http_main_confs());
            http_core_plugin_conf.plugin_handle_logs.clone()
        };

        for plugin_handle_log in &*plugin_handle_logs.get().await {
            (plugin_handle_log)(stream_info.clone()).await?;
        }
    }

    if ret.is_some() {
        log::debug!("ret:{:?}", ret.as_ref().unwrap());
    }

    //延迟服务器关闭, 让客户端接收完缓冲区, 在进行socket关闭
    if is_ups_close && stream_so_singer_time > 0 {
        tokio::time::sleep(tokio::time::Duration::from_millis(
            stream_so_singer_time as u64,
        ))
        .await;
    }

    if is_close {
        return Ok(());
    }

    if ret.is_some() {
        let ret = ret.unwrap();
        let _ = ret?;
    }

    Ok(())
}

pub async fn stream_info_parse(
    stream_timeout: StreamTimeout,
    stream_info: Share<StreamInfo>,
) -> Result<(bool, bool)> {
    let stream_info = &mut stream_info.get_mut();
    let mut is_close = false;
    let mut is_ups_close = false;
    let client_stream_flow_info = stream_info.client_stream_flow_info.clone();
    let upstream_stream_flow_info = stream_info.upstream_stream_flow_info.clone();
    let mut client_stream_flow_info = client_stream_flow_info.get_mut();
    let mut upstream_stream_flow_info = upstream_stream_flow_info.get_mut();

    #[cfg(feature = "anyerror")]
    let mut is_error = false;
    #[cfg(feature = "anyerror")]
    {
        if client_stream_flow_info.read == 0
            || client_stream_flow_info.write == 0
            || upstream_stream_flow_info.read == 0
            || upstream_stream_flow_info.write == 0
        {
            is_error = true;
        } else {
            let tunnel_hello_size = stream_info.protocol_hello_size as i64;
            let ups_write = if tunnel_hello_size > 0 {
                if upstream_stream_flow_info.write >= tunnel_hello_size
                    && upstream_stream_flow_info.write > client_stream_flow_info.read
                {
                    upstream_stream_flow_info.write - tunnel_hello_size
                } else {
                    upstream_stream_flow_info.write
                }
            } else {
                upstream_stream_flow_info.write
            };

            let tunnel_hello_size = stream_info.client_protocol_hello_size as i64;
            let client_read = if tunnel_hello_size > 0 {
                if client_stream_flow_info.read >= tunnel_hello_size
                    && client_stream_flow_info.read > upstream_stream_flow_info.write
                {
                    client_stream_flow_info.read - tunnel_hello_size
                } else {
                    client_stream_flow_info.read
                }
            } else {
                client_stream_flow_info.read
            };

            if client_read != ups_write {
                is_error = true;
            }

            if client_stream_flow_info.write != upstream_stream_flow_info.read {
                is_error = true;
            }
        }

        if stream_info.is_discard_timeout || stream_info.is_discard_flow {
            is_error = false;
        }

        if is_error {
            log::error!(
                "session_id:{}, client_stream_flow_info.read:{} , upstream_stream_flow_info.write:{}, \
                client_stream_flow_info.write:{} , upstream_stream_flow_info.read:{}, \
                   protocol_hello_size:{}, client_protocol_hello_size:{}",
                stream_info.request_id,
                client_stream_flow_info.read,
                upstream_stream_flow_info.write,
                client_stream_flow_info.write,
                upstream_stream_flow_info.read,
                stream_info.protocol_hello_size,
                stream_info.client_protocol_hello_size,
            )
        }
    }

    if stream_info.err_status == ErrStatus::Ok
        || stream_info.err_status == ErrStatus::ClientProtoErr
    {
        if stream_timeout.timeout_num.load(Ordering::Relaxed) >= 4 {
            stream_info.is_timeout_exit = true;
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
            if client_stream_flow_info.err_time_millis >= upstream_stream_flow_info.err_time_millis
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
                if err_status_200.is_ups_err() {
                    is_ups_close = true;
                }
            } else if err == stream_flow::StreamFlowErr::ReadClose {
                is_close = true;
                stream_info.err_status_str = Some(err_status_200.read_close());
                if err_status_200.is_ups_err() {
                    is_ups_close = true;
                }
            } else if err == stream_flow::StreamFlowErr::WriteReset {
                is_close = true;
                stream_info.err_status_str = Some(err_status_200.write_reset());
                if err_status_200.is_ups_err() {
                    is_ups_close = true;
                }
            } else if err == stream_flow::StreamFlowErr::ReadReset {
                is_close = true;
                stream_info.err_status_str = Some(err_status_200.read_reset());
                if err_status_200.is_ups_err() {
                    is_ups_close = true;
                }
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
        let upstream_connect_flow_info = stream_info.upstream_connect_flow_info.clone();
        let upstream_connect_flow_info = upstream_connect_flow_info.get();
        if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteTimeout {
            stream_info.err_status = ErrStatus::GatewayTimeout;
        } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteReset {
            stream_info.err_status_str = Some(stream_info::UPS_CONN_RESET.clone());
        } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteErr {
            stream_info.err_status_str = Some(stream_info::UPS_CONN_ERR.clone());
        }
    }

    #[cfg(feature = "anyerror")]
    if is_error {
        is_close = false;
    }

    Ok((is_close, is_ups_close))
}

pub async fn read_timeout(
    stream_flow_info: ArcMutex<StreamFlowInfo>,
    stream_info: Share<StreamInfo>,
    stream_timeout: StreamTimeout,
    timeout_millis: Arc<AtomicI64>,
) -> Result<()> {
    let (mut read_timeout, mut read) = {
        let stream_flow_info = stream_flow_info.get();
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
            let read_timeout = stream_flow_info.get().read_timeout;
            read_timeout
        };
        if read_timeout <= 0 || read_timeout == u64::MAX {
            read_timeout = 10;
            continue;
        }

        if stream_info.get().is_discard_timeout {
            continue;
        }

        let curr_read = { stream_flow_info.get().read };
        if curr_read <= read {
            let stream_flow_info = stream_flow_info.get();
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
    stream_flow_info: ArcMutex<StreamFlowInfo>,
    stream_info: Share<StreamInfo>,
    stream_timeout: StreamTimeout,
    timeout_millis: Arc<AtomicI64>,
) -> Result<()> {
    let (mut write_timeout, mut write) = {
        let stream_flow_info = stream_flow_info.get();
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
            let write_timeout = stream_flow_info.get().write_timeout;
            write_timeout
        };
        if write_timeout <= 0 || write_timeout == u64::MAX {
            write_timeout = 10;
            continue;
        }

        if stream_info.get().is_discard_timeout {
            continue;
        }

        let curr_write = { stream_flow_info.get().write };
        if curr_write <= write {
            let stream_flow_info = stream_flow_info.get();
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

pub async fn debug_print_access_log(
    debug_print_access_log_time: u64,
    stream_info: Share<StreamInfo>,
) -> Result<()> {
    let timeout = if debug_print_access_log_time <= 0 {
        10
    } else {
        debug_print_access_log_time
    };

    let mut scc: ShareRw<StreamConfigContext> = ShareRw::default();

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
        if debug_print_access_log_time <= 0 || debug_print_access_log_time == u64::MAX {
            continue;
        }
        if stream_info.get().is_discard_flow {
            continue;
        }
        if stream_info.get().scc.is_some() {
            if scc.is_none() {
                scc = stream_info.get().scc.clone();
            }
            if let Err(e) = AccessLog::debug_access_log(stream_info.clone())
                .await
                .map_err(|e| anyhow!("err:StreamStream::debug_access_log => e:{}", e))
            {
                log::error!("err:{}", e);
                continue;
            }
        }
    }
}

pub async fn debug_print_stream_flow(
    debug_print_stream_flow_time: u64,
    stream_info: Share<StreamInfo>,
) -> Result<()> {
    let timeout = if debug_print_stream_flow_time <= 0 {
        10
    } else {
        debug_print_stream_flow_time
    };
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
        if debug_print_stream_flow_time <= 0 || debug_print_stream_flow_time == u64::MAX {
            continue;
        }
        if stream_info.get().is_discard_flow {
            continue;
        }

        let stream_info = stream_info.get();
        let client_stream_flow_info = stream_info.client_stream_flow_info.get();
        let upstream_stream_flow_info = stream_info.upstream_stream_flow_info.get();

        log::info!(
            "stream_flow_sec:{} {} {} {}",
            client_stream_flow_info.read,
            upstream_stream_flow_info.write,
            upstream_stream_flow_info.read,
            client_stream_flow_info.write
        )
    }
}
