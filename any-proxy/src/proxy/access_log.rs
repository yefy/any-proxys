use crate::config::config_toml::AccessConfig;
use crate::proxy::proxy::AccessContext;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_var;
use crate::stream::server::ServerStreamInfo;
use crate::util::var::Var;
use crate::Protocol7;
use any_base::typ::Share;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

pub struct AccessLog {}

impl AccessLog {
    pub async fn parse_config_access_log(access: &Vec<AccessConfig>) -> Result<Vec<AccessContext>> {
        let mut access_map = HashMap::new();
        let stream_info_test = StreamInfo::new(
            Arc::new(ServerStreamInfo {
                protocol7: Protocol7::Tcp,
                remote_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
                local_addr: Some(SocketAddr::from(([127, 0, 0, 1], 18080))),
                domain: None,
                is_tls: false,
            }),
            false,
            None,
            0,
            0,
            0,
            false,
        );

        let mut access_context = Vec::new();
        for access in access {
            if !access.access_log {
                continue;
            }

            let ret: Result<Var> = async {
                let access_format_vars = Var::new(&access.access_format, "-")
                    .map_err(|e| anyhow!("err:Var::new => e:{}", e))?;
                let mut access_format_vars_test = Var::copy(&access_format_vars)
                    .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
                access_format_vars_test.for_each(|var| {
                    let var_name = Var::var_name(var);
                    let value = stream_var::find(var_name, &stream_info_test)
                        .map_err(|e| anyhow!("err:stream_var.find => e:{}", e))?;
                    Ok(value)
                })?;
                let _ = access_format_vars_test
                    .join()
                    .map_err(|e| anyhow!("err:access_format_var.join => e:{}", e))?;
                Ok(access_format_vars)
            }
            .await;
            let access_format_vars = ret.map_err(|e| {
                anyhow!(
                    "err:access_format => access_format:{}, e:{}",
                    access.access_format,
                    e
                )
            })?;

            let ret: Result<()> = async {
                {
                    //就是创建下文件 啥都不干
                    let _ = std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&access.access_log_file);
                }
                let path = Path::new(&access.access_log_file);
                let canonicalize = path
                    .canonicalize()
                    .map_err(|e| anyhow!("err:path.canonicalize() => e:{}", e))?;
                let path = canonicalize
                    .to_str()
                    .ok_or(anyhow!("err:{}", access.access_log_file))?
                    .to_string();
                let access_log_file = match access_map.get(&path).cloned() {
                    Some(access_log_file) => access_log_file,
                    None => {
                        let access_log_file = std::fs::OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(&access.access_log_file)
                            .map_err(|e| {
                                anyhow!("err::open {} => e:{}", access.access_log_file, e)
                            })?;
                        let access_log_file = Arc::new(access_log_file);
                        access_map.insert(path, access_log_file.clone());
                        access_log_file
                    }
                };

                access_context.push(AccessContext {
                    access_format_vars,
                    access_log_file,
                });
                Ok(())
            }
            .await;
            ret.map_err(|e| {
                anyhow!(
                    "err:access_log_file => access_log_file:{}, e:{}",
                    access.access_log_file,
                    e
                )
            })?;
        }
        Ok(access_context)
    }

    pub async fn access_log(stream_info: Share<StreamInfo>) -> Result<()> {
        let stream_info = stream_info.get_mut();
        use crate::config::http_access_log;
        let scc = stream_info.scc.get();
        let http_access_log_conf = http_access_log::currs_conf(scc.http_server_confs());

        for (index, access) in http_access_log_conf.access.iter().enumerate() {
            if access.access_log {
                let access_context = &http_access_log_conf.access_context[index];
                let mut access_format_var = Var::copy(&access_context.access_format_vars)
                    .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
                access_format_var.for_each(|var| {
                    let var_name = Var::var_name(var);
                    let value = stream_var::find(var_name, &stream_info);
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

    pub async fn debug_access_log(stream_info: Share<StreamInfo>) -> Result<()> {
        let stream_info = stream_info.get();
        use crate::config::http_access_log;
        let scc = stream_info.scc.get();
        let http_access_log_conf = http_access_log::currs_conf(scc.http_server_confs());

        for (index, _) in http_access_log_conf.access.iter().enumerate() {
            let access_context = &http_access_log_conf.access_context[index];
            let mut access_format_var = Var::copy(&access_context.access_format_vars)
                .map_err(|e| anyhow!("err:Var::copy => e:{}", e))?;
            access_format_var.for_each(|var| {
                let var_name = Var::var_name(var);
                let value = stream_var::find(var_name, &stream_info);
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
}
