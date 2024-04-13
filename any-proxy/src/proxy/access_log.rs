use crate::config::config_toml::AccessConfig;
use crate::proxy::proxy::AccessContext;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_var;
use crate::util::var::Var;
use crate::wasm::run_wasm_plugin;
use crate::wasm::WasmHost;
use any_base::typ::Share;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

pub struct AccessLog {}

impl AccessLog {
    pub async fn parse_config_access_log(access: &Vec<AccessConfig>) -> Result<Vec<AccessContext>> {
        let mut access_map = HashMap::new();
        use crate::util::default_config::VAR_STREAM_INFO;

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
                    let value = stream_var::find(var_name, &VAR_STREAM_INFO)
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
        use crate::config::net_access_log;
        let scc = stream_info.scc.clone();
        let net_access_log_conf = net_access_log::curr_conf(scc.net_curr_conf());

        for (index, access) in net_access_log_conf.access.iter().enumerate() {
            if access.access_log {
                if access.is_err_access_log && !stream_info.is_err {
                    continue;
                }
                let access_context = &net_access_log_conf.access_context[index];
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
        use crate::config::net_access_log;
        let scc = stream_info.scc.clone();
        let net_access_log_conf = net_access_log::curr_conf(scc.net_curr_conf());

        for (index, _) in net_access_log_conf.access.iter().enumerate() {
            let access_context = &net_access_log_conf.access_context[index];
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

    pub async fn wasm_access_log(stream_info: Share<StreamInfo>) -> Result<()> {
        if stream_info.get().scc.is_none() {
            return Ok(());
        }
        let scc = stream_info.get().scc.clone();
        log::trace!(
            "session_id:{}, wasm_access_log",
            stream_info.get().session_id
        );
        use crate::config::net_access_log;
        use crate::config::net_core_wasm;
        let conf = net_access_log::curr_conf(scc.net_curr_conf());

        if conf.wasm_plugin_confs.is_none() {
            return Ok(());
        }
        let wasm_plugin_confs = conf.wasm_plugin_confs.as_ref().unwrap();
        let net_core_wasm_conf = net_core_wasm::main_conf(scc.ms()).await;
        for wasm_plugin_conf in &wasm_plugin_confs.wasm {
            let wasm_plugin = net_core_wasm_conf.get_wasm_plugin(&wasm_plugin_conf.wasm_path)?;
            let plugin = WasmHost::new(stream_info.clone());
            let ret = run_wasm_plugin(&wasm_plugin_conf.wasm_config, plugin, &wasm_plugin).await;
            if let Err(e) = &ret {
                log::error!("wasm_access_log:{}", e);
            }
        }
        Ok(())
    }
}
