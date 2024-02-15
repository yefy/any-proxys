use super::anyproxy_group;
use crate::anyproxy::anyproxy_group::AnyproxyGroup;
use crate::util::default_config;
use crate::util::signal;
use any_base::executor_local_spawn;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::module::module;
use anyhow::anyhow;
use anyhow::Result;
use structopt::StructOpt;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, StructOpt)]
pub struct ArgsConfig {
    #[structopt(long = "sig", short = "s")]
    pub signal: Option<String>,
    #[structopt(long = "sigfile")]
    pub signal_file: Option<String>,
    #[structopt(long, short = "t")]
    pub check_config: bool,
    #[structopt(long, short = "c")]
    pub config: Option<String>,
    #[structopt(long = "hot")]
    pub hot: Option<String>,
}
/*
命令:anyproxy -s quit 正常关闭，可设置超时时间
命令:anyproxy -s stop 快速关闭
命令:anyproxy -s reload 配置热加载
命令:anyproxy -s reinit 重新分配线程，并配置热加载
命令:anyproxy -t 配置正确性检查
命令:anyproxy -c 指定配置文件路径
命令:anyproxy --hot 指定pid文件路径
命令:anyproxy --sigfile quit 正常关闭，可设置超时时间
命令:anyproxy --sigfile stop 快速关闭
命令:anyproxy --sigfile reload 配置热加载
命令:anyproxy --sigfile reinit 重新分配线程，并配置热加载
 */

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
    executor: ExecutorLocalSpawn,
    group_version: i32,
}

impl Anyproxy {
    pub fn new(executor: ExecutorLocalSpawn) -> Result<Anyproxy> {
        Ok(Anyproxy {
            executor,
            group_version: 0,
        })
    }

    pub fn set_hot_pid(hot: &str) -> Result<bool> {
        let pid = std::fs::read_to_string(hot)
            .map_err(|e| anyhow::anyhow!("err:hot anyproxy not run => e:{}", e))?;
        let _ = pid
            .parse::<usize>()
            .map_err(|e| anyhow::anyhow!("err:hot pid => e:{}", e))?;
        default_config::HOT_PID.set(pid);
        return Ok(false);
    }

    pub fn run_linux_signal(sig: &str, pid: &str) -> Result<String> {
        use std::process::Command;
        let mut cmd = Command::new("/bin/kill");
        cmd.arg("-s");
        cmd.arg(sig);
        cmd.arg(pid);

        let output = cmd.output()?;
        let output_str = String::from_utf8(output.stdout)?;
        Ok(output_str)
    }

    pub fn run_signal(sig: &str) -> Result<bool> {
        let pid = Anyproxy::read_pid_file().map_err(|e| anyhow!("err:pid is nil => e:{}", e))?;
        let sig_linux = match sig {
            "quit" => "QUIT",
            "stop" => "TERM",
            "reload" => "HUP",
            "reinit" => "USR1",
            _ => {
                log::error!("err:not support signal:{}", sig);
                return Err(anyhow!("err:not support signal:{}", sig));
            }
        };

        match Self::run_linux_signal(sig_linux, &pid) {
            Ok(output) => {
                log::info!("signal pid {} info:{}", pid, output);
            }
            Err(e) => {
                log::error!("err:signal pid :{} => e:{}", pid, e);
            }
        }
        Ok(true)
    }

    pub fn run_hot_pid() -> Result<()> {
        if default_config::HOT_PID.is_none() {
            return Ok(());
        }
        let pid = unsafe { default_config::HOT_PID.take() };
        match Self::run_linux_signal("QUIT", &pid) {
            Ok(output) => {
                log::info!("hot pid {} info:{}", pid, output);
            }
            Err(e) => {
                log::error!("err:hot pid :{} => e:{}", pid, e);
            }
        }
        Ok(())
    }

    pub fn parse_args(arg_config: &ArgsConfig) -> Result<bool> {
        if arg_config.signal.is_some() {
            let signal = arg_config.signal.as_ref().unwrap();
            #[cfg(unix)]
            {
                if Anyproxy::run_signal(signal)
                    .map_err(|e| anyhow::anyhow!("err:signal_file => e:{}", e))?
                {
                    return Ok(true);
                }
            }
            log::error!(
                "err:system not support, please use: anyproxy --sigfile {}",
                signal
            );
            return Ok(true);
        }

        if arg_config.signal_file.is_some() {
            if Anyproxy::write_signal(arg_config.signal_file.as_ref().unwrap())
                .map_err(|e| anyhow::anyhow!("err:signal_file => e:{}", e))?
            {
                return Ok(true);
            }
        }

        if arg_config.config.is_some() {
            let config = arg_config.config.clone().unwrap();
            default_config::ANYPROXY_CONF_FULL_PATH.set(config);
        }

        if arg_config.hot.is_some() {
            #[cfg(unix)]
            if true {
                return Anyproxy::set_hot_pid(arg_config.hot.as_ref().unwrap());
            }
            log::error!("err:system not support hot");
            return Ok(true);
        }

        if arg_config.check_config {
            executor_local_spawn::_block_on(1, 1, move |executor| async move {
                AnyproxyGroup::check(0, executor.executors())
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
            Anyproxy::remove_signal_file();
            log::info!("anyproxy end");
        };

        log::info!("anyproxy start");
        Anyproxy::remove_signal_file();
        Anyproxy::remove_pid_file();
        Anyproxy::create_pid_file()
            .map_err(|e| anyhow!("err:Anyproxy::create_pid_file => e:{}", e))?;

        let file_name = { default_config::ANYPROXY_CONF_FULL_PATH.get().clone() };
        let mut ms = module::Modules::new(None);
        ms.parse_module_config(&file_name, None)
            .await
            .map_err(|e| anyhow!("err:file_name:{} => e:{}", file_name, e))?;

        use crate::config::common_core;
        let common_conf = common_core::main_conf(&ms).await;
        let shutdown_timeout = common_conf.shutdown_timeout;

        let mut anyproxy_group: Option<anyproxy_group::AnyproxyGroup> = Some(
            anyproxy_group::AnyproxyGroup::new(self.group_version_add(), ms)?,
        );
        anyproxy_group
            .as_mut()
            .unwrap()
            .start()
            .await
            .map_err(|e| anyhow!("err:anyproxy start => e:{}", e))?;

        #[cfg(unix)]
        Self::run_hot_pid()?;

        loop {
            let anyproxy_state = Anyproxy::read_signal().await;

            match anyproxy_state {
                AnyproxyState::Skip => {
                    continue;
                }
                AnyproxyState::Quit => {
                    self.async_anyproxy_group_stop(
                        anyproxy_group.unwrap(),
                        false,
                        shutdown_timeout,
                    )
                    .await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Stop => {
                    self.async_anyproxy_group_stop(anyproxy_group.unwrap(), true, shutdown_timeout)
                        .await;
                    log::info!("signal: quit");
                    break;
                }
                AnyproxyState::Reload => {
                    anyproxy_group
                        .as_mut()
                        .unwrap()
                        .reload(self.executor.executors())
                        .await;
                    continue;
                }
                AnyproxyState::Check => {
                    let _ = anyproxy_group::AnyproxyGroup::check(
                        self.group_version,
                        self.executor.executors(),
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
                        let ms = anyproxy_group.as_ref().unwrap().ms();
                        let mut new_anyproxy_group =
                            anyproxy_group::AnyproxyGroup::new(self.group_version_add(), ms)
                                .map_err(|e| {
                                    anyhow!("err:anyproxy_group::AnyproxyGroup => e:{}", e)
                                })?;
                        if let Err(e) = new_anyproxy_group
                            .start()
                            .await
                            .map_err(|e| anyhow!("err:anyproxy start => e:{}", e))
                        {
                            log::error!(
                                "err:reload => group_version:{}, e:{}",
                                new_anyproxy_group.group_version(),
                                e
                            );
                            self.async_anyproxy_group_stop(
                                new_anyproxy_group,
                                false,
                                shutdown_timeout,
                            )
                            .await;
                            continue;
                        }

                        self.async_anyproxy_group_stop(
                            anyproxy_group.take().unwrap(),
                            false,
                            shutdown_timeout,
                        )
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
        shutdown_timeout: u64,
    ) {
        self.executor._start(
            #[cfg(feature = "anyspawn-count")]
            None,
            move |_| async move {
                anyproxy_group
                    .stop(is_fast_shutdown, shutdown_timeout)
                    .await;
                Ok(())
            },
        );
    }

    pub async fn wait_anyproxy_groups(&mut self) -> Result<()> {
        log::info!("anyproxy wait_anyproxy_groups");
        self.executor.wait("anyproxy wait anyproxy_groups").await?;
        Ok(())
    }

    pub fn create_pid_file() -> Result<()> {
        let anyproxy_pid = unsafe { libc::getpid() };
        log::info!("anyproxy pid:{}", anyproxy_pid);
        std::fs::write(
            default_config::ANYPROXY_PID_FULL_PATH.get().as_str(),
            format!("{}", anyproxy_pid),
        )
        .map_err(|e| anyhow!("err:std::fs::write => e:{}", e))?;
        Ok(())
    }
    pub fn remove_pid_file() {
        let _ = std::fs::remove_file(default_config::ANYPROXY_PID_FULL_PATH.get().as_str());
    }

    pub fn read_pid_file() -> Result<String> {
        let pid = std::fs::read_to_string(default_config::ANYPROXY_PID_FULL_PATH.get().as_str())
            .map_err(|e| anyhow!("err:std::fs::read_to_string => e:{}", e))?;
        Ok(pid)
    }

    pub fn remove_signal_file() {
        let _ = std::fs::remove_file(default_config::ANYPROXY_SIGNAL_FULL_PATH.get().as_str());
    }

    pub fn load_signal_file() -> AnyproxyState {
        let sig =
            match std::fs::read_to_string(default_config::ANYPROXY_SIGNAL_FULL_PATH.get().as_str())
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
            log::error!("err:pid is nil");
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
            std::fs::write(
                default_config::ANYPROXY_SIGNAL_FULL_PATH.get().as_str(),
                sig,
            )
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
            sig = signal::stop() =>  {
                if sig {
                    return AnyproxyState::Stop;
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
             _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
               return Anyproxy::load_signal_file();
            },
            else => {
                return AnyproxyState::Skip;
            },
        }
    }
}
