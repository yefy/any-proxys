use super::anyproxy_group;
use crate::anyproxy::anyproxy_group::AnyproxyGroup;
use crate::config::config;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::util::default_config;
use crate::util::signal;
use crate::Tunnels;
use any_base::executor_local_spawn;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_tunnel::client;
use any_tunnel::server;
use any_tunnel2::client as client2;
use any_tunnel2::server as server2;
use any_tunnel2::tunnel as tunnel2;
use anyhow::anyhow;
use anyhow::Result;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Arc;
use structopt::StructOpt;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, StructOpt)]
pub struct ArgsConfig {
    #[structopt(long = "sig", short = "s")]
    pub signal: Option<String>,
    #[structopt(long, short = "t")]
    pub check_config: bool,
}

impl ArgsConfig {
    /// Load configs from args.
    pub fn load_from_args() -> Self {
        ArgsConfig::from_args()
    }
}

/*
PIDFile=/usr/local/anyproxy/logs/anyproxy.pid
ExecStartPre=/usr/local/anyproxy/anyproxy -t
ExecStart=/usr/local/anyproxy/anyproxy
ExecReload=/bin/kill -s HUP $MAINPID
ExecStop=/bin/kill -s QUIT $MAINPID
 */

/*
火焰图
https://github.com/flamegraph-rs/flamegraph
[profile.release]
debug = true

yum install perf

cargo install flamegraph

flamegraph -o out.svg ./anyproxy
cargo flamegraph -o out.svg --example some_example --features some_features
*/

#[derive(Debug, Clone)]
pub enum AnyproxyState {
    Skip = 0,
    Quit = 1,   //正常停止 /bin/kill -s QUIT $MAINPID 或 ctrl_c(SIGINT)
    Stop = 2,   //快速停止 /bin/kill -s TERM $MAINPID
    Reload = 3, //刷新配置 /bin/kill -s HUP $MAINPID
    Reinit = 4, //重新创建线程和刷新配置 /bin/kill -s USR1 $MAINPID
    Check = 5,  //检查配置 /bin/kill -s USR2 $MAINPID
}

pub struct Anyproxy {
    executor_local_spawn: ExecutorLocalSpawn,
    group_version: i32,
}

impl Anyproxy {
    pub fn new(executor_local_spawn: ExecutorLocalSpawn) -> Result<Anyproxy> {
        Ok(Anyproxy {
            executor_local_spawn,
            group_version: 0,
        })
    }

    pub fn parse_args(arg_config: &ArgsConfig) -> Result<bool> {
        if arg_config.signal.is_some() {
            if Anyproxy::write_signal(arg_config.signal.as_ref().unwrap())
                .map_err(|e| anyhow::anyhow!("err:signal => e:{}", e))?
            {
                return Ok(true);
            }
        }

        if arg_config.check_config {
            executor_local_spawn::_block_on(move |executor_local_spawn| async move {
                AnyproxyGroup::check(0, executor_local_spawn.executors())
                    .await
                    .map_err(|e| anyhow::anyhow!("err:check => e:{}", e))?;
                Ok(())
            })?;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn group_version_add(&mut self) -> i32 {
        self.group_version = self.group_version + 1;
        self.group_version
    }

    pub async fn start(&mut self) -> Result<()> {
        scopeguard::defer! {
            Anyproxy::remove_pid_file();
            Anyproxy::remove_signal_file();
            log::info!("anyproxy end");
        };

        log::info!("anyproxy start");
        Anyproxy::create_pid_file()
            .map_err(|e| anyhow!("err:Anyproxy::create_pid_file => e:{}", e))?;
        Anyproxy::remove_signal_file();

        let config = config::Config::new().map_err(|e| anyhow!("err:Config::new() => e:{}", e))?;

        let peer_stream_max_len = Arc::new(AtomicUsize::new(config.tunnel2.tunnel2_max_connect));
        let client2 = tunnel2::Tunnel::start_thread(config.tunnel2.tunnel2_worker_thread).await;
        let client2 = client2::Client::new(client2, peer_stream_max_len.clone());

        let server2 = tunnel2::Tunnel::start_thread(config.tunnel2.tunnel2_worker_thread).await;
        let server2 = server2::Server::new(server2);

        let client = client::Client::new();
        let server = server::Server::new();

        let session_id = Arc::new(AtomicU64::new(100000));

        #[cfg(feature = "anyproxy-ebpf")]
        let mut ebpf_group = any_ebpf::EbpfGroup::new(config.common.cpu_affinity, 0)?;
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = ebpf_group.start(config.common.is_open_ebpf_log).await?;
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = Arc::new(ebpf_add_sock_hash);

        let tunnels = Tunnels {
            client,
            server,
            client2,
            server2,
        };

        let mut anyproxy_group: Option<anyproxy_group::AnyproxyGroup> = Some(
            anyproxy_group::AnyproxyGroup::new(self.group_version_add())?,
        );
        anyproxy_group
            .as_mut()
            .unwrap()
            .start(
                Some(config),
                peer_stream_max_len.clone(),
                tunnels.clone(),
                #[cfg(feature = "anyproxy-ebpf")]
                ebpf_add_sock_hash.clone(),
                session_id.clone(),
            )
            .await
            .map_err(|e| anyhow!("err:anyproxy start => e:{}", e))?;

        loop {
            let anyproxy_state = Anyproxy::read_signal().await;

            match anyproxy_state {
                AnyproxyState::Skip => {
                    continue;
                }
                AnyproxyState::Quit => {
                    self.async_anyproxy_group_stop(anyproxy_group.unwrap(), false)
                        .await;
                    #[cfg(feature = "anyproxy-ebpf")]
                    ebpf_group.stop().await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Stop => {
                    self.async_anyproxy_group_stop(anyproxy_group.unwrap(), true)
                        .await;
                    #[cfg(feature = "anyproxy-ebpf")]
                    ebpf_group.stop().await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Reload => {
                    anyproxy_group
                        .as_ref()
                        .unwrap()
                        .reload(
                            peer_stream_max_len.clone(),
                            self.executor_local_spawn.executors(),
                        )
                        .await;
                    continue;
                }
                AnyproxyState::Check => {
                    let _ = anyproxy_group::AnyproxyGroup::check(
                        self.group_version,
                        self.executor_local_spawn.executors(),
                    )
                    .await;
                    continue;
                }
                AnyproxyState::Reinit => {
                    #[cfg(windows)]
                    {
                        log::error!("err:window not support Reinit");
                        continue;
                    }
                    #[cfg(unix)]
                    {
                        let mut new_anyproxy_group = anyproxy_group::AnyproxyGroup::new(
                            self.group_version_add(),
                        )
                        .map_err(|e| anyhow!("err:anyproxy_group::AnyproxyGroup => e:{}", e))?;
                        if let Err(e) = new_anyproxy_group
                            .start(
                                None,
                                peer_stream_max_len.clone(),
                                tunnels.clone(),
                                #[cfg(feature = "anyproxy-ebpf")]
                                ebpf_add_sock_hash.clone(),
                                session_id.clone(),
                            )
                            .await
                            .map_err(|e| anyhow!("err:anyproxy start => e:{}", e))
                        {
                            log::error!(
                                "err:reload => group_version:{}, e:{}",
                                new_anyproxy_group.group_version(),
                                e
                            );
                            self.async_anyproxy_group_stop(new_anyproxy_group, false)
                                .await;
                            continue;
                        }

                        self.async_anyproxy_group_stop(anyproxy_group.take().unwrap(), false)
                            .await;
                        anyproxy_group = Some(new_anyproxy_group);
                    }
                }
            }
        }

        self.wait_anyproxy_groups().await?;

        Ok(())
    }

    pub async fn async_anyproxy_group_stop(
        &mut self,
        anyproxy_group: anyproxy_group::AnyproxyGroup,
        is_fast_shutdown: bool,
    ) {
        self.executor_local_spawn._start(move |_| async move {
            anyproxy_group.stop(is_fast_shutdown).await;
            Ok(())
        });
    }

    pub async fn wait_anyproxy_groups(&mut self) -> Result<()> {
        log::info!("anyproxy wait_anyproxy_groups");
        self.executor_local_spawn.stop(true, 10).await;
        Ok(())
    }

    pub fn create_pid_file() -> Result<()> {
        let anyproxy_pid = unsafe { libc::getpid() };
        log::info!("anyproxy pid:{}", anyproxy_pid);
        std::fs::write(
            default_config::ANYPROXY_PID_FULL_PATH.as_str(),
            format!("{}", anyproxy_pid),
        )
        .map_err(|e| anyhow!("err:std::fs::write => e:{}", e))?;
        Ok(())
    }
    pub fn remove_pid_file() {
        let _ = std::fs::remove_file(default_config::ANYPROXY_PID_FULL_PATH.as_str());
    }

    pub fn read_pid_file() -> Result<String> {
        let pid = std::fs::read_to_string(default_config::ANYPROXY_PID_FULL_PATH.as_str())
            .map_err(|e| anyhow!("err:std::fs::read_to_string => e:{}", e))?;
        Ok(pid)
    }

    pub fn remove_signal_file() {
        let _ = std::fs::remove_file(default_config::ANYPROXY_SIGNAL_FULL_PATH.as_str());
    }

    pub fn load_signal_file() -> AnyproxyState {
        let sig = match std::fs::read_to_string(default_config::ANYPROXY_SIGNAL_FULL_PATH.as_str())
        {
            Err(_) => "".to_string(),
            Ok(sig) => sig.trim().to_string(),
        };
        Anyproxy::remove_signal_file();

        match &sig[..] {
            "quit" => AnyproxyState::Quit,
            "stop" => AnyproxyState::Stop,
            "reload" => AnyproxyState::Reload,
            "reinit" => {
                #[cfg(windows)]
                {
                    log::error!("err:window not support signal:{}", sig);
                    return AnyproxyState::Skip;
                }
                #[cfg(unix)]
                AnyproxyState::Reinit
            }
            "" => AnyproxyState::Skip,
            _ => {
                log::error!("err:not support signal:{}", sig);
                return AnyproxyState::Skip;
            }
        }
    }

    pub fn write_signal(sig: &str) -> Result<bool> {
        let pid = Anyproxy::read_pid_file();
        if pid.is_err() {
            log::error!("err:anyproxy not run");
        }
        let is_sig = match sig {
            "quit" => true,
            "stop" => true,
            "reload" => true,
            "reinit" => {
                #[cfg(windows)]
                {
                    log::error!("err:window not support signal:{}", sig);
                    return Err(anyhow!("err:window not support signal:{}", sig));
                }
                #[cfg(unix)]
                true
            }
            "" => false,
            _ => {
                log::error!("err:not support signal:{}", sig);
                return Err(anyhow!("err:not support signal:{}", sig));
            }
        };
        if is_sig {
            std::fs::write(default_config::ANYPROXY_SIGNAL_FULL_PATH.as_str(), sig)
                .map_err(|e| anyhow!("err:std::fs::write => e:{}", e))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn read_signal() -> AnyproxyState {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() =>  {
                return AnyproxyState::Quit;
            },
            sig = signal::quit() =>  {
                if sig {
                    return AnyproxyState::Quit;
                } else {
                    return AnyproxyState::Skip;
                }
            },
            sig = signal::hup() =>  {
                if sig {
                    return AnyproxyState::Reload;
                } else {
                    return AnyproxyState::Skip;
                }
            },
            sig = signal::user1() =>  {
                if sig {
                    return AnyproxyState::Reinit;
                } else {
                    return AnyproxyState::Skip;
                }
            },
            sig = signal::user2() =>  {
                if sig {
                    return AnyproxyState::Check;
                } else {
                    return AnyproxyState::Skip;
                }
            },
             _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
               return Anyproxy::load_signal_file();
            },
            else => {
                return AnyproxyState::Skip;
            },
        }
    }
}
