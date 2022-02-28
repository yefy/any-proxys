use super::anyproxy_group;
use crate::anyproxy::anyproxy_group::AnyproxyGroup;
use crate::config::config;
use crate::util::default_config;
use crate::util::signal;
use any_tunnel::client;
use any_tunnel::server;
use any_tunnel2::client as client2;
use any_tunnel2::server as server2;
use any_tunnel2::tunnel as tunnel2;
use awaitgroup::WaitGroup;
use futures_util::task::LocalSpawnExt;
use std::sync::atomic::AtomicUsize;
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
    executor: async_executors::TokioCt,
    group_version: i32,
    wait_group: WaitGroup,
}

impl Anyproxy {
    pub fn new(executor: async_executors::TokioCt) -> anyhow::Result<Anyproxy> {
        Ok(Anyproxy {
            executor,
            group_version: 0,
            wait_group: WaitGroup::new(),
        })
    }

    pub fn parse_args(arg_config: &ArgsConfig) -> anyhow::Result<bool> {
        if arg_config.signal.is_some() {
            if Anyproxy::write_signal(arg_config.signal.as_ref().unwrap())? {
                return Ok(true);
            }
        }

        if arg_config.check_config {
            let executor = async_executors::TokioCtBuilder::new().build().unwrap();
            let ret = executor
                .clone()
                .block_on(async { AnyproxyGroup::check(0).await });
            if let Err(e) = ret {
                log::error!("err:async_main => e:{}", e);
                return Err(anyhow::anyhow!("err:async_main => e:{}", e));
            }
            return Ok(true);
        }

        Ok(false)
    }

    pub fn group_version_add(&mut self) -> i32 {
        self.group_version = self.group_version + 1;
        self.group_version
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        scopeguard::defer! {
            Anyproxy::remove_pid_file();
            Anyproxy::remove_signal_file();
            log::info!("anyproxy end");
        };

        log::info!("anyproxy start");
        Anyproxy::create_pid_file()?;
        Anyproxy::remove_signal_file();

        let config =
            config::Config::new().map_err(|e| anyhow::anyhow!("err:Config::new() => e:{}", e))?;

        let peer_stream_max_len = Arc::new(AtomicUsize::new(config.tunnel2.tunnel2_max_connect));
        let tunnel2_client =
            tunnel2::Tunnel::start_thread(config.tunnel2.tunnel2_worker_thread).await;
        let tunnel2_clients = client2::Client::new(tunnel2_client, peer_stream_max_len.clone());

        let tunnel2_server =
            tunnel2::Tunnel::start_thread(config.tunnel2.tunnel2_worker_thread).await;
        let tunnel2_servers = server2::Server::new(tunnel2_server);

        let tunnel_clients = client::Client::new();
        let tunnel_servers = server::Server::new();

        let mut anyproxy_group: Option<anyproxy_group::AnyproxyGroup> = Some(
            anyproxy_group::AnyproxyGroup::new(self.group_version_add())?,
        );
        anyproxy_group
            .as_mut()
            .unwrap()
            .start(
                tunnel_clients.clone(),
                tunnel_servers.clone(),
                peer_stream_max_len.clone(),
                tunnel2_clients.clone(),
                tunnel2_servers.clone(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("err:anyproxy start => e:{}", e))?;

        loop {
            let anyproxy_state = Anyproxy::read_signal().await;

            match anyproxy_state {
                AnyproxyState::Skip => {
                    continue;
                }
                AnyproxyState::Quit => {
                    self.async_anyproxy_group_stop(anyproxy_group.unwrap(), false)
                        .await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Stop => {
                    self.async_anyproxy_group_stop(anyproxy_group.unwrap(), true)
                        .await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Reload => {
                    anyproxy_group
                        .as_ref()
                        .unwrap()
                        .reload(peer_stream_max_len.clone())
                        .await;
                    continue;
                }
                AnyproxyState::Check => {
                    let _ = anyproxy_group::AnyproxyGroup::check(self.group_version).await;
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
                        let mut new_anyproxy_group =
                            anyproxy_group::AnyproxyGroup::new(self.group_version_add())?;
                        if let Err(e) = new_anyproxy_group
                            .start(
                                tunnel_clients.clone(),
                                tunnel_servers.clone(),
                                peer_stream_max_len.clone(),
                                tunnel2_clients.clone(),
                                tunnel2_servers.clone(),
                            )
                            .await
                            .map_err(|e| anyhow::anyhow!("err:anyproxy start => e:{}", e))
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

        self.wait_anyproxy_groups().await;

        Ok(())
    }

    pub async fn async_anyproxy_group_stop(
        &self,
        mut anyproxy_group: anyproxy_group::AnyproxyGroup,
        is_fast_shutdown: bool,
    ) {
        let worker = self.wait_group.worker().add();
        self.executor
            .spawn_local(async move {
                scopeguard::defer! {
                    worker.done();
                }
                anyproxy_group.stop(is_fast_shutdown).await;
            })
            .unwrap_or_else(|e| log::error!("{}", e));
    }

    pub async fn wait_anyproxy_groups(&self) {
        log::info!("anyproxy wait_anyproxy_groups");
        self.wait_group.wait().await;
    }

    pub fn create_pid_file() -> anyhow::Result<()> {
        let anyproxy_pid = unsafe { libc::getpid() };
        log::info!("anyproxy pid:{}", anyproxy_pid);
        std::fs::write(
            default_config::ANYPROXY_PID_FULL_PATH.as_str(),
            format!("{}", anyproxy_pid),
        )?;
        Ok(())
    }
    pub fn remove_pid_file() {
        let _ = std::fs::remove_file(default_config::ANYPROXY_PID_FULL_PATH.as_str());
    }

    pub fn read_pid_file() -> anyhow::Result<String> {
        let pid = std::fs::read_to_string(default_config::ANYPROXY_PID_FULL_PATH.as_str())?;
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

    pub fn write_signal(sig: &str) -> anyhow::Result<bool> {
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
                    return Err(anyhow::anyhow!("err:window not support signal:{}", sig));
                }
                #[cfg(unix)]
                true
            }
            "" => false,
            _ => {
                log::error!("err:not support signal:{}", sig);
                return Err(anyhow::anyhow!("err:not support signal:{}", sig));
            }
        };
        if is_sig {
            std::fs::write(default_config::ANYPROXY_SIGNAL_FULL_PATH.as_str(), sig)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn read_signal() -> AnyproxyState {
        loop {
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
                //signal test
                // _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                //         return AnyproxyState::User1;
                // }
                else => {
                    return AnyproxyState::Skip;
                },
            }
        }
    }
}
