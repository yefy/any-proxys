pub mod anyproxy;
pub mod config;
pub mod ebpf;
pub mod macros;
pub mod protopack;
pub mod proxy;
pub mod quic;
pub mod ssl;
pub mod stream;
pub mod tcp;
pub mod tunnel;
pub mod tunnel2;
pub mod upstream;
pub mod util;
pub mod wasm;

use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::module::module::Modules;
use any_base::typ::ArcMutex;
use any_tunnel2::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

pub fn default_wasm_plug_conf_is_open() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmPluginConf {
    #[serde(default = "default_wasm_plug_conf_is_open")]
    pub is_open: bool,
    pub wasm_path: String,
    pub wasm_main_config: String,
    pub wasm_main_timeout_config: Option<String>,
    pub wasm_main_ext1_config: Option<String>,
    pub wasm_main_ext2_config: Option<String>,
    pub wasm_main_ext3_config: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmPluginConfs {
    pub wasm: Vec<WasmPluginConf>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Protocol7 {
    Tcp,
    Ssl,
    Quic,
    TunnelTcp,
    TunnelSsl,
    TunnelQuic,
    Tunnel2Tcp,
    Tunnel2Ssl,
    Tunnel2Quic,
}

impl Protocol7 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol7::Tcp => "tcp".to_string(),
            Protocol7::Ssl => "ssl".to_string(),
            Protocol7::Quic => "quic".to_string(),
            Protocol7::TunnelTcp => "tunnelTcp".to_string(),
            Protocol7::TunnelSsl => "tunnelSsl".to_string(),
            Protocol7::TunnelQuic => "tunnelQuic".to_string(),
            Protocol7::Tunnel2Tcp => "tunnel2Tcp".to_string(),
            Protocol7::Tunnel2Ssl => "tunnel2Ssl".to_string(),
            Protocol7::Tunnel2Quic => "tunnel2Quic".to_string(),
        }
    }

    pub fn from_string(name: &str) -> Result<Protocol7> {
        match name {
            "tcp" => Ok(Protocol7::Tcp),
            "ssl" => Ok(Protocol7::Ssl),
            "quic" => Ok(Protocol7::Quic),
            "tunnelTcp" => Ok(Protocol7::TunnelTcp),
            "tunnelSsl" => Ok(Protocol7::TunnelSsl),
            "tunnelQuic" => Ok(Protocol7::TunnelQuic),
            "tunnel2Tcp" => Ok(Protocol7::Tunnel2Tcp),
            "tunnel2Ssl" => Ok(Protocol7::Tunnel2Ssl),
            "tunnel2Quic" => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:Protocol7 nil"));
            }
        }
    }

    pub fn to_tunnel_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::TunnelTcp),
            Protocol7::Ssl => Ok(Protocol7::TunnelSsl),
            Protocol7::Quic => Ok(Protocol7::TunnelQuic),
            _ => {
                return Err(anyhow!("err:to_tunnel_protocol7"));
            }
        }
    }

    pub fn to_tunnel2_protocol7(&self) -> Result<Protocol7> {
        match self {
            Protocol7::Tcp => Ok(Protocol7::Tunnel2Tcp),
            Protocol7::Ssl => Ok(Protocol7::Tunnel2Ssl),
            Protocol7::Quic => Ok(Protocol7::Tunnel2Quic),
            _ => {
                return Err(anyhow!("err:to_tunnel2_protocol7"));
            }
        }
    }

    pub fn to_protocol4(&self) -> Protocol4 {
        match self {
            Protocol7::Tcp => Protocol4::TCP,
            Protocol7::Ssl => Protocol4::TCP,
            Protocol7::Quic => Protocol4::UDP,
            Protocol7::TunnelTcp => Protocol4::TCP,
            Protocol7::TunnelSsl => Protocol4::TCP,
            Protocol7::TunnelQuic => Protocol4::UDP,
            Protocol7::Tunnel2Tcp => Protocol4::TCP,
            Protocol7::Tunnel2Ssl => Protocol4::TCP,
            Protocol7::Tunnel2Quic => Protocol4::UDP,
        }
    }

    pub fn is_tunnel(&self) -> bool {
        match self {
            Protocol7::Tcp => false,
            Protocol7::Ssl => false,
            Protocol7::Quic => false,
            Protocol7::TunnelTcp => true,
            Protocol7::TunnelSsl => true,
            Protocol7::TunnelQuic => true,
            Protocol7::Tunnel2Tcp => true,
            Protocol7::Tunnel2Ssl => true,
            Protocol7::Tunnel2Quic => true,
        }
    }
}

pub enum Protocol77 {
    Http,
    Http2,
    WebSocket,
    Https,
    Https2,
    WebSockets,
}

impl Protocol77 {
    pub fn to_string(&self) -> String {
        match self {
            Protocol77::Http => "http".to_string(),
            Protocol77::Http2 => "http2".to_string(),
            Protocol77::WebSocket => "webSocket".to_string(),
            Protocol77::Https => "https".to_string(),
            Protocol77::Https2 => "https2".to_string(),
            Protocol77::WebSockets => "webSockets".to_string(),
        }
    }

    pub fn from_string(name: &str) -> Result<Protocol77> {
        match name {
            "http" => Ok(Protocol77::Http),
            "http2" => Ok(Protocol77::Http2),
            "webSocket" => Ok(Protocol77::WebSocket),
            "https" => Ok(Protocol77::Https),
            "https2" => Ok(Protocol77::Https2),
            "webSockets" => Ok(Protocol77::WebSockets),
            _ => {
                return Err(anyhow!("err:Protocol77 nil"));
            }
        }
    }
}

pub enum Error {
    //继续进行
    Ok,
    //结束当前循环
    Break,
    //结束所有循环
    Finish,
    //错误退出请求
    Error,
    //退出请求
    Return,
    Ext1,
    Ext2,
    Ext3,
}

pub struct ExecutorLocalSpawnMs {
    ms: Modules,
    executor: ExecutorLocalSpawn,
    ms_executor: ExecutorLocalSpawn,
    shutdown_timeout: u64,
}

impl ExecutorLocalSpawnMs {
    pub fn new(
        ms: Modules,
        executor: ExecutorLocalSpawn,
        ms_executor: ExecutorLocalSpawn,
        shutdown_timeout: u64,
    ) -> Self {
        ExecutorLocalSpawnMs {
            ms,
            executor,
            ms_executor,
            shutdown_timeout,
        }
    }

    pub fn executor(&self) -> ExecutorLocalSpawn {
        self.ms_executor.clone()
    }
}

impl Drop for ExecutorLocalSpawnMs {
    fn drop(&mut self) {
        let ms = self.ms.clone();
        let ms_executor = self.ms_executor.clone();
        let shutdown_timeout = self.shutdown_timeout;
        let _ = self.executor.clone()._start_and_free(move |_| async move {
            log::debug!(target: "ms", "ms session_id:{}, count:{} => drop ExecutorLocalSpawnMs", ms.session_id(), ms.count());
            ms_executor
                .stop("ExecutorLocalSpawnMs", true, shutdown_timeout)
                .await;
            log::debug!(target: "ms", "ms session_id:{}, count:{} => drop ExecutorLocalSpawnMs ms_executor", ms.session_id(), ms.count());

            use crate::config::port_core;
            let port_core_conf = port_core::main_conf_mut(&ms).await;
            port_core_conf.port_config_listen_map.clear();

            log::debug!(target: "ms", "ms session_id:{}, count:{} => drop ExecutorLocalSpawnMs port_core_conf", ms.session_id(), ms.count());

            use crate::config::domain_core;
            let domain_core_conf = domain_core::main_conf_mut(&ms).await;
            for (_, v) in domain_core_conf.domain_config_listen_map.iter_mut() {
                v.domain_config_context_map.clear()
            }
            domain_core_conf.domain_config_listen_merge_map.clear();
            domain_core_conf.domain_config_listen_map.clear();
            log::debug!(target: "ms", "ms session_id:{}, count:{} => drop ExecutorLocalSpawnMs domain_core_conf", ms.session_id(), ms.count());
            Ok(())
        });
    }
}

pub struct ModulesExecutor {
    pub ms: Modules,
    pub ms_executor: ArcMutex<ExecutorLocalSpawnMs>,
}

impl ModulesExecutor {
    pub fn new(
        ms: Modules,
        executor: ExecutorLocalSpawn,
        ms_executor: ExecutorLocalSpawn,
        shutdown_timeout: u64,
    ) -> Self {
        ModulesExecutor {
            ms: ms.clone(),
            ms_executor: ArcMutex::new(ExecutorLocalSpawnMs::new(
                ms,
                executor,
                ms_executor,
                shutdown_timeout,
            )),
        }
    }

    pub fn del_ms_executor(&self) {
        if self.ms_executor.is_some() {
            self.ms_executor.set_nil();
        }
    }
}
