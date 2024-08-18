use super::access_log::AccessLog;
use super::proxy;
use super::stream_info;
use super::stream_info::ErrStatus;
use super::stream_info::StreamInfo;
use crate::proxy::stream_info::ErrStatusInfo;
use crate::proxy::StreamTimeout;
use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use any_base::stream_flow;
use any_base::typ::{ArcMutex, Share};
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

pub async fn do_start(
    mut start_stream: impl proxy::Stream,
    stream_info: &Share<StreamInfo>,
    mut shutdown_thread_rx: broadcast::Receiver<bool>,
) -> Result<()> {
    let start_time = Instant::now();
    let stream_timeout = StreamTimeout::new();

    let client_stream_flow_info = stream_info.get().client_stream_flow_info.clone();
    let upstream_stream_flow_info = stream_info.get().upstream_stream_flow_info.clone();
    let ret: Option<Result<()>> = async {
        tokio::select! {
            biased;
            _ret = start_stream.do_start(stream_info.clone()) => {
                let _ret = _ret.map_err(|e| anyhow!("err:do_start => request_id:{}, e:{}",
                    stream_info.get().request_id, e));
                return Some(_ret);
            }
            _ret = read_timeout(
                client_stream_flow_info.clone(),
                &stream_info,
                stream_timeout.clone(),
                stream_timeout.client_read_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = read_timeout(
                upstream_stream_flow_info.clone(),
                &stream_info,
                stream_timeout.clone(),
                stream_timeout.ups_read_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = write_timeout(
                client_stream_flow_info.clone(),
                &stream_info,
                stream_timeout.clone(),
                stream_timeout.client_write_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = write_timeout(
                upstream_stream_flow_info.clone(),
                &stream_info,
                stream_timeout.clone(),
                stream_timeout.ups_write_timeout_millis.clone(),
            ) => {
                return Some(_ret);
            }
            _ret = debug_print_access_log(&stream_info) => {
                return Some(_ret);
            }
             _ret = debug_print_stream_flow(&stream_info) => {
                return Some(_ret);
            }
            _ = shutdown_thread_rx.recv() => {
                stream_info.get_mut().err_status = ErrStatus::SERVER_ERR;
                return None;
            }
            else => {
                stream_info.get_mut().err_status = ErrStatus::SERVER_ERR;
                return None;
            }
        }
    }
    .await;

    let (debug_is_open_print, scc, stream_so_singer_time) = {
        let session_time = start_time.elapsed().as_secs_f32();
        let mut stream_info = stream_info.get_mut();
        stream_info.session_time = session_time;
        (
            stream_info.debug_is_open_print,
            stream_info.scc.clone(),
            stream_info.stream_so_singer_time,
        )
    };

    if debug_is_open_print {
        let stream_info = stream_info.get();
        log::info!(
            "session_id:{}, request_id:{}---{:?}:do_start end",
            stream_info.session_id,
            stream_info.request_id,
            stream_info.server_stream_info.local_addr,
        );
    }

    let (is_close, is_ups_close) = stream_info_parse(stream_timeout, &stream_info, ret.is_some())
        .await
        .map_err(|e| anyhow!("err:stream_connect_parse => e:{}", e))?;
    if scc.is_some() {
        use crate::config::net_core_plugin;
        //___wait___
        //let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_core_conf());
        let net_core_plugin_conf = net_core_plugin::currs_conf(scc.net_main_confs());

        for plugin_handle_log in &*net_core_plugin_conf.plugin_handle_logs.get().await {
            (plugin_handle_log)(stream_info.clone()).await?;
        }
    }

    if ret.is_some() {
        let ret = ret.as_ref().unwrap();
        if let Err(e) = ret {
            let stream_info = stream_info.get();
            log::debug!(target: "main",
                "session_id:{}, request_id:{}, err:{}",
                stream_info.session_id,
                stream_info.request_id,
                e
            );
        }
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
        if let Err(e) = ret {
            let stream_info = stream_info.get();
            return Err(anyhow!(
                "session_id:{}, request_id:{}, err:{}",
                stream_info.session_id,
                stream_info.request_id,
                e
            ));
        }
    }

    Ok(())
}

pub async fn stream_info_parse(
    stream_timeout: StreamTimeout,
    stream_info: &Share<StreamInfo>,
    is_ret_err: bool,
) -> Result<(bool, bool)> {
    let stream_info = &mut stream_info.get_mut();
    let mut is_error = false;
    let mut is_close = false;
    let mut is_ups_close = false;
    let client_stream_flow_info = stream_info.client_stream_flow_info.clone();
    let upstream_stream_flow_info = stream_info.upstream_stream_flow_info.clone();
    let mut client_stream_flow_info = client_stream_flow_info.get_mut();
    let mut upstream_stream_flow_info = upstream_stream_flow_info.get_mut();
    log::debug!(target: "ext3", "err_status:{}, client_stream_flow_info.err:{:?}, upstream_stream_flow_info.err:{:?}", stream_info.err_status, client_stream_flow_info.err, upstream_stream_flow_info.err);
    if stream_info.ssc_upload.is_some() {
        if stream_info.ssc_download.is_some() {
            let ssc = &stream_info.ssc_download;
            let ssd = ssc.ssd.get();
            // if ssd.total_read_size == 0 && ssd.total_write_size == 0 {
            //     is_error = true;
            // }
            if ssd.stream_cache_size > ssc.cs.max_stream_cache_size {
                is_error = true;
            }
            if ssd.tmp_file_size > ssc.cs.max_tmp_file_size {
                is_error = true;
            }
        }

        let ssc = &stream_info.ssc_upload;
        let ssd = ssc.ssd.get();
        if ssd.stream_cache_size > ssc.cs.max_stream_cache_size {
            is_error = true;
        }
        if ssd.tmp_file_size > ssc.cs.max_tmp_file_size {
            is_error = true;
        }
    }

    #[cfg(feature = "anyerror")]
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
            stream_info.upstream_protocol_hello_size,
            stream_info.client_protocol_hello_size,
        )
    }

    if client_stream_flow_info.err == StreamFlowErr::Init
        && upstream_stream_flow_info.err == StreamFlowErr::Init
    {
        is_error = true;
    }

    if stream_info.err_status < 500 || stream_info.err_status > 599 {
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

        let client_err = ErrStatusInfo::new(
            client_stream_flow_info.err.clone(),
            Box::new(stream_info::ErrStatusClient {}),
        );
        let upstream_err = ErrStatusInfo::new(
            upstream_stream_flow_info.err.clone(),
            Box::new(stream_info::ErrStatusUpstream {}),
        );

        if client_err.is_close || (client_err.err == StreamFlowErr::Init && upstream_err.is_close) {
            is_error = false;
            is_close = true;
            if upstream_err.is_ups_close {
                is_ups_close = true;
            }
        } else {
            is_error = true;
            is_close = false;
            is_ups_close = false;
        }

        let errs = if client_stream_flow_info.err_time_millis
            <= upstream_stream_flow_info.err_time_millis
        {
            vec![client_err, upstream_err]
        } else {
            vec![upstream_err, client_err]
        };

        if errs[0].err_str.is_some() && errs[1].err_str.is_some() {
            stream_info.err_status_str = Some(ArcString::new(format!(
                "({})({})",
                errs[0].err_str.as_ref().unwrap().as_str(),
                errs[1].err_str.as_ref().unwrap().as_str()
            )));
        } else if errs[0].err_str.is_some() {
            stream_info.err_status_str = Some(ArcString::new(format!(
                "({})(nil)",
                errs[0].err_str.as_ref().unwrap().as_str()
            )));
        } else if errs[1].err_str.is_some() {
            stream_info.err_status_str = Some(ArcString::new(format!(
                "(nil)({})",
                errs[1].err_str.as_ref().unwrap().as_str()
            )));
        } else {
            stream_info.err_status_str = Some(ArcString::new(format!("(nil)(nil)")));
        }
    } else if stream_info.err_status == ErrStatus::SERVICE_UNAVAILABLE {
        is_error = true;
        let upstream_connect_flow_info = stream_info.upstream_connect_flow_info.clone();
        let upstream_connect_flow_info = upstream_connect_flow_info.get();
        if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteTimeout {
            stream_info.err_status = ErrStatus::GATEWAY_TIMEOUT;
        } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteReset {
            stream_info.err_status_str = Some(stream_info::UPS_CONN_RESET.clone());
        } else if upstream_connect_flow_info.err == stream_flow::StreamFlowErr::WriteErr {
            stream_info.err_status_str = Some(stream_info::UPS_CONN_ERR.clone());
        }
    }

    if !is_close && is_ret_err {
        is_error = true;
    }

    if stream_info.is_discard_flow {
        is_error = false;
    }
    log::debug!(target: "ext3", "is_error:{}, is_close:{}, is_ups_close:{}", is_error, is_close, is_ups_close);
    stream_info.is_err = is_error;
    Ok((is_close, is_ups_close))
}

pub async fn read_timeout(
    stream_flow_info: ArcMutex<StreamFlowInfo>,
    stream_info: &Share<StreamInfo>,
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
    stream_info: &Share<StreamInfo>,
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

pub async fn debug_print_access_log(stream_info: &Share<StreamInfo>) -> Result<()> {
    loop {
        let (debug_print_access_log_time, is_discard_flow) = {
            let stream_info = stream_info.get();
            (
                stream_info.debug_print_access_log_time,
                stream_info.is_discard_flow,
            )
        };

        let timeout = if debug_print_access_log_time <= 0 {
            10
        } else {
            if is_discard_flow {
                10
            } else {
                debug_print_access_log_time
            }
        };

        tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
        if debug_print_access_log_time <= 0
            || debug_print_access_log_time == u64::MAX
            || is_discard_flow
        {
            continue;
        }

        if stream_info.get().scc.is_some() {
            if let Err(e) = AccessLog::debug_access_log(&stream_info)
                .await
                .map_err(|e| anyhow!("err:StreamStream::debug_access_log => e:{}", e))
            {
                log::error!("err:{}", e);
                continue;
            }
        }
    }
}

pub async fn debug_print_stream_flow(stream_info: &Share<StreamInfo>) -> Result<()> {
    loop {
        let (debug_print_stream_flow_time, is_discard_flow) = {
            let stream_info = stream_info.get();
            (
                stream_info.debug_print_stream_flow_time,
                stream_info.is_discard_flow,
            )
        };

        let timeout = if debug_print_stream_flow_time <= 0 {
            10
        } else {
            if is_discard_flow {
                10
            } else {
                debug_print_stream_flow_time
            }
        };

        tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
        if debug_print_stream_flow_time <= 0
            || debug_print_stream_flow_time == u64::MAX
            || is_discard_flow
        {
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
