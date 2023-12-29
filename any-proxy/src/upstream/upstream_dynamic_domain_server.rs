use super::UpstreamData;
use crate::util;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;

pub struct UpstreamDynamicDomainServer {
    executors: ExecutorsLocal,
    ups_data: ArcMutex<UpstreamData>,
    ///vector中第几个域名的索引
    index: usize,
}

impl UpstreamDynamicDomainServer {
    pub fn spawn_local(executors: ExecutorsLocal, ups_data: ArcMutex<UpstreamData>, index: usize) {
        executors._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
            move |executors| async move {
                let dynamic_domain_server =
                    UpstreamDynamicDomainServer::new(executors, ups_data, index)
                        .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                dynamic_domain_server
                    .start()
                    .await
                    .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                Ok(())
            },
        );
    }
    pub fn new(
        executors: ExecutorsLocal,
        ups_data: ArcMutex<UpstreamData>,
        index: usize,
    ) -> Result<UpstreamDynamicDomainServer> {
        Ok(UpstreamDynamicDomainServer {
            executors,
            ups_data,
            index,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let (interval, timeout, host, mut addrs, ups_config_name) = {
            let ups_data = self.ups_data.get_mut();
            let ups_dynamic_domain = ups_data.ups_dynamic_domains[self.index].clone();
            let ups_dynamic_domain = ups_dynamic_domain.get();
            let dynamic_domain = ups_dynamic_domain.dynamic_domain.as_ref().unwrap();
            (
                dynamic_domain.interval,
                dynamic_domain.timeout,
                ups_dynamic_domain.host.clone(),
                ups_dynamic_domain.addrs.clone(),
                ups_data.ups_config.name.clone(),
            )
        };
        scopeguard::defer! {
            log::debug!("stop run_ups_dynamic_domains ups_name:[{}], host:{}", ups_config_name, host);
        }
        log::debug!(
            "start run_ups_dynamic_domains ups_name:[{}], host:{}",
            ups_config_name,
            host
        );
        let mut shutdown_thread_rx = self.executors.context.shutdown_thread_tx.subscribe();
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval as u64)) => {
                }
                _ =  shutdown_thread_rx.recv() => {
                    return Ok(());
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }

            let curr_addrs =
                util::util::lookup_hosts(tokio::time::Duration::from_secs(timeout as u64), &host)
                    .await
                    .map_err(|e| anyhow!("err:lookup_host => host:{} e:{}", host, e));
            let mut curr_addrs = match curr_addrs {
                Ok(curr_addrs) => curr_addrs,
                Err(err) => {
                    log::info!("upstream_server.rs host:{}, err:{}", host, err);
                    continue;
                }
            };
            curr_addrs.sort_by(|a, b| a.partial_cmp(b).unwrap());

            if addrs.len() == curr_addrs.len() {
                let mut is_change = false;
                for (i, v) in addrs.iter().enumerate() {
                    if v != &curr_addrs[i] {
                        is_change = true;
                    }
                }
                if !is_change {
                    continue;
                }
            }

            log::info!("name:{}, {:?} => {:?}", ups_config_name, addrs, curr_addrs);
            addrs = curr_addrs.clone();
            {
                let mut ups_data = self.ups_data.get_mut();
                ups_data.is_dynamic_domain_change = true;
                ups_data.ups_dynamic_domains[self.index].get_mut().addrs = curr_addrs.clone();
            }
        }
    }
}
