use super::UpstreamData;
use crate::util;
use crate::Executors;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WorkerInner;
use futures_util::task::LocalSpawnExt;
use std::cell::RefCell;
use std::rc::Rc;

pub struct UpstreamDynamicDomainServer {
    executors: Executors,
    ups_data: Rc<RefCell<Option<UpstreamData>>>,
    index: usize,
}

impl UpstreamDynamicDomainServer {
    pub fn spawn_local(
        worker: WorkerInner,
        executors: Executors,
        ups_data: Rc<RefCell<Option<UpstreamData>>>,
        index: usize,
    ) {
        executors
            .executor
            .clone()
            .spawn_local(async move {
                scopeguard::defer! {
                    worker.done();
                }

                let ret: Result<()> = async {
                    let dynamic_domain_server =
                        UpstreamDynamicDomainServer::new(executors.clone(), ups_data, index)
                            .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    dynamic_domain_server
                        .start()
                        .await
                        .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                    Ok(())
                }
                .await;
                ret.unwrap_or_else(|e| log::error!("err:Port.start => e:{}", e));
            })
            .unwrap_or_else(|e| log::error!("{}", e));
    }
    pub fn new(
        executors: Executors,
        ups_data: Rc<RefCell<Option<UpstreamData>>>,
        index: usize,
    ) -> Result<UpstreamDynamicDomainServer> {
        Ok(UpstreamDynamicDomainServer {
            executors,
            ups_data,
            index,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let (interval, timeout, host, mut addrs) = {
            let ups_data = self.ups_data.borrow();
            let ups_dynamic_domain = &ups_data.as_ref().unwrap().ups_dynamic_domains[self.index];
            let dynamic_domain = ups_dynamic_domain.dynamic_domain.as_ref().unwrap();
            (
                dynamic_domain.interval,
                dynamic_domain.timeout,
                ups_dynamic_domain.host.clone(),
                ups_dynamic_domain.addrs.clone(),
            )
        };
        scopeguard::defer! {
        log::info!("stop run_ups_dynamic_domains ups_name:[{}], host:{}", self.ups_data.borrow().as_ref().unwrap().ups_config.name, host);
        }
        log::info!(
            "start run_ups_dynamic_domains ups_name:[{}], host:{}",
            self.ups_data.borrow().as_ref().unwrap().ups_config.name,
            host
        );
        let mut shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();
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

            let mut ups_data = self.ups_data.borrow_mut().take().unwrap();
            log::info!(
                "name:{}, {:?} => {:?}",
                ups_data.ups_config.name,
                addrs,
                curr_addrs
            );
            addrs = curr_addrs.clone();

            ups_data.is_change = true;
            ups_data.ups_dynamic_domains[self.index].addrs = curr_addrs.clone();

            *self.ups_data.borrow_mut() = Some(ups_data)
        }
    }
}
