use super::stream_connect;
use super::stream_connect::ErrStatus;
use super::stream_connect::StreamConnect;
use super::stream_var;
use crate::config::config_toml;
use crate::proxy;
use crate::stream::stream_flow;
use crate::util::var;
use std::io::Write;
use std::rc::Rc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct StreamStream {}

impl StreamStream {
    pub fn work_time(name: &str, stream_work_times: bool, stream_connect: &mut StreamConnect) {
        if stream_work_times {
            let stream_work_time = stream_connect
                .stream_work_time
                .as_ref()
                .unwrap()
                .elapsed()
                .as_secs_f32();
            stream_connect
                .stream_work_times
                .borrow_mut()
                .push((name.to_string(), stream_work_time));
            stream_connect.stream_work_time = Some(Instant::now());
        }
    }

    pub async fn stream_to_stream(
        stream_cache_size: usize,
        stream_work_times: bool,
        stream_connect: &mut StreamConnect,
        client_stream: &mut stream_flow::StreamFlow,
        upstream_stream: &mut stream_flow::StreamFlow,
    ) -> anyhow::Result<()> {
        let mut cli_vec: Vec<u8> = vec![0u8; stream_cache_size];
        let cli_slice = cli_vec.as_mut_slice();
        let mut ups_vec: Vec<u8> = vec![0u8; stream_cache_size];
        let ups_slice = ups_vec.as_mut_slice();
        let mut cli_pack_id = 0;
        let mut ups_pack_id = 0;

        let mut is_upstream_slice_first = true;
        loop {
            let read_ret: anyhow::Result<(bool, usize)> = async {
                loop {
                    tokio::select! {
                        biased;
                        cli_ret = client_stream.read(cli_slice) => {
                            let n = cli_ret.map_err(|e| anyhow::anyhow!("err:client_stream.read => e:{}", e))?;
                            return Ok((true, n));
                        }
                        upstream_ret = upstream_stream.read(ups_slice) => {
                            let n = upstream_ret.map_err(|e| anyhow::anyhow!("err:upstream_stream.read => e:{}", e))?;
                            return Ok((false, n));
                        }
                        else => {
                            return Err(anyhow::anyhow!(
                            "err:stream_to_stream select close"
                        ));
                        }
                    }
                }
            }
            .await;

            let (is_cli, n) = read_ret?;
            if is_cli {
                cli_pack_id += 1;
                log::trace!("client -> upstream cli_pack_id:{}", cli_pack_id);
                // log::trace!(
                //     "upstream -> client:{}",
                //     String::from_utf8_lossy(&cli_slice[..n])
                // );
                upstream_stream
                    .write_all(&cli_slice[..n])
                    .await
                    .map_err(|e| anyhow::anyhow!("err:upstream_stream.write_all => e:{}", e))?;
            } else {
                if is_upstream_slice_first {
                    is_upstream_slice_first = false;
                    StreamStream::work_time(
                        "is_upstream_slice_first",
                        stream_work_times,
                        stream_connect,
                    );
                }
                ups_pack_id += 1;
                log::trace!("upstream -> client ups_pack_id:{}, n:{}", ups_pack_id, n);
                // log::trace!(
                //     "client -> upstream:{}",
                //     String::from_utf8_lossy(&ups_slice[..n])
                // );
                client_stream
                    .write_all(&ups_slice[..n])
                    .await
                    .map_err(|e| anyhow::anyhow!("err:client_stream.write_all => e:{}", e))?;
            }
        }
    }

    pub async fn stream_connect_parse(stream_connect: &mut StreamConnect) -> anyhow::Result<bool> {
        let mut is_close = false;
        if stream_connect.err_status == ErrStatus::Ok {
            let client_stream_info = stream_connect.client_stream_info.lock().unwrap();
            let upstream_stream_info = stream_connect.upstream_stream_info.lock().unwrap();

            let (err, err_status_200): (
                stream_flow::StreamFlowErr,
                Option<Box<dyn stream_connect::ErrStatus200>>,
            ) = if client_stream_info.err != stream_flow::StreamFlowErr::Init {
                (
                    client_stream_info.err.clone(),
                    Some(Box::new(stream_connect::ErrStatusClient {})),
                )
            } else if upstream_stream_info.err != stream_flow::StreamFlowErr::Init {
                (
                    upstream_stream_info.err.clone(),
                    Some(Box::new(stream_connect::ErrStatusUpstream {})),
                )
            } else {
                (stream_flow::StreamFlowErr::Init, None)
            };

            if err != stream_flow::StreamFlowErr::Init {
                let err_status_200 = err_status_200.as_ref().unwrap();
                if err == stream_flow::StreamFlowErr::WriteClose {
                    is_close = true;
                    stream_connect.err_status_str = Some(err_status_200.write_close());
                } else if err == stream_flow::StreamFlowErr::ReadClose {
                    is_close = true;
                    stream_connect.err_status_str = Some(err_status_200.read_close());
                } else if err == stream_flow::StreamFlowErr::WriteReset {
                    is_close = true;
                    stream_connect.err_status_str = Some(err_status_200.write_reset());
                } else if err == stream_flow::StreamFlowErr::ReadReset {
                    is_close = true;
                    stream_connect.err_status_str = Some(err_status_200.read_reset());
                } else if err == stream_flow::StreamFlowErr::WriteTimeout {
                    stream_connect.err_status_str = Some(err_status_200.write_timeout());
                } else if err == stream_flow::StreamFlowErr::ReadTimeout {
                    stream_connect.err_status_str = Some(err_status_200.read_timeout());
                } else if err == stream_flow::StreamFlowErr::WriteErr {
                    stream_connect.err_status_str = Some(err_status_200.write_err());
                } else if err == stream_flow::StreamFlowErr::ReadErr {
                    stream_connect.err_status_str = Some(err_status_200.read_err());
                }
            }
        } else if stream_connect.err_status == ErrStatus::ServiceUnavailable {
            let upstream_connect_info = stream_connect.upstream_connect_info.borrow();
            if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteTimeout {
                stream_connect.err_status = ErrStatus::GatewayTimeout;
            } else if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteReset {
                stream_connect.err_status_str = Some(stream_connect::UPS_CONN_RESET.to_string());
            } else if upstream_connect_info.err == stream_flow::StreamFlowErr::WriteErr {
                stream_connect.err_status_str = Some(stream_connect::UPS_CONN_ERR.to_string());
            }
        }

        Ok(is_close)
    }

    pub async fn access_log(
        access: &Vec<config_toml::AccessConfig>,
        access_context: &Vec<proxy::proxy::AccessContext>,
        stream_var: &Rc<stream_var::StreamVar>,
        stream_connect: &mut StreamConnect,
    ) -> anyhow::Result<()> {
        for (index, access) in access.iter().enumerate() {
            if access.access_log {
                let access_context = &access_context[index];
                let mut access_format_var = var::Var::copy(&access_context.access_format_vars)?;
                access_format_var.for_each(|var| {
                    let var_name = var::Var::var_name(var);
                    let value = stream_var.find(var_name, stream_connect);
                    match value {
                        Err(e) => {
                            log::error!("{}", anyhow::anyhow!("{}", e));
                            Ok(None)
                        }
                        Ok(value) => Ok(value),
                    }
                })?;

                let mut access_log_data = access_format_var.join()?;

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
                            anyhow::anyhow!(
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
