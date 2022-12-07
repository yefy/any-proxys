use super::upstream_config::UpstreamConfig;
use super::upstream_dynamic_domain_server::UpstreamDynamicDomainServer;
use super::upstream_heartbeat_server::UpstreamHeartbeatServer;
use super::UpstreamData;
use crate::Executors;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::Worker;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct UpstreamServer {
    executors: Executors,
    work_group: Worker,
    ups_data: Rc<RefCell<Option<UpstreamData>>>,
}

impl UpstreamServer {
    pub fn new(
        executors: Executors,
        work_group: Worker,
        ups_data: Rc<RefCell<Option<UpstreamData>>>,
    ) -> Result<UpstreamServer> {
        Ok(UpstreamServer {
            executors,
            work_group,
            ups_data,
        })
    }

    pub async fn start(&self) -> Result<()> {
        log::info!(
            "start run_ups_dynamic_domains ups_name:[{}]",
            self.ups_data.borrow().as_ref().unwrap().ups_config.name
        );
        self.run_ups_dynamic_domains().await;
        log::info!(
            "start run_ups_heartbeats ups_name:[{}]",
            self.ups_data.borrow().as_ref().unwrap().ups_config.name
        );
        self.run_ups_heartbeats().await;
        log::info!(
            "start ups_name:[{}]",
            self.ups_data.borrow().as_ref().unwrap().ups_config.name
        );
        self.run().await
    }

    pub async fn run_ups_dynamic_domains(&self) {
        let ups_data = self.ups_data.borrow();
        let ups_data = ups_data.as_ref().unwrap();
        for (index, ups_dynamic_domain) in ups_data.ups_dynamic_domains.iter().enumerate() {
            if ups_dynamic_domain.dynamic_domain.is_none() {
                continue;
            }
            UpstreamDynamicDomainServer::spawn_local(
                self.work_group.worker().add(),
                self.executors.clone(),
                self.ups_data.clone(),
                index,
            );
        }
    }

    pub async fn run_ups_heartbeats(&self) {
        let ups_data = self.ups_data.borrow();
        let ups_data = ups_data.as_ref().unwrap();
        for (_, ups_heartbeat) in ups_data.ups_heartbeats.iter().enumerate() {
            if ups_heartbeat.borrow().heartbeat.is_none() {
                continue;
            }
            UpstreamHeartbeatServer::spawn_local(
                self.work_group.worker().add(),
                self.executors.clone(),
                self.ups_data.clone(),
                ups_heartbeat.borrow().index,
            );
        }
    }

    pub async fn run(&self) -> Result<()> {
        let mut shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {
                }
                _ =  shutdown_thread_rx.recv() => {
                    return Ok(());
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }

            let mut ups_data = self.ups_data.borrow_mut().take().unwrap();
            if !ups_data.is_change {
                *self.ups_data.borrow_mut() = Some(ups_data);
                continue;
            }

            ups_data.is_change = false;
            let mut domain_addrs = HashMap::new();
            for (_, ups_dynamic_domain) in ups_data.ups_dynamic_domains.iter().enumerate() {
                for (_, addr) in ups_dynamic_domain.addrs.iter().enumerate() {
                    let key = format!("{}_{}", ups_dynamic_domain.index, addr);
                    domain_addrs.insert(key, true);
                }
            }
            let mut del_indexs = Vec::new();
            for (index, ups_heartbeat) in ups_data.ups_heartbeats.iter().enumerate() {
                let key = format!(
                    "{}_{}",
                    ups_heartbeat.borrow().domain_index,
                    ups_heartbeat.borrow().addr
                );
                if domain_addrs.get(&key).is_none() {
                    del_indexs.push(index);
                }
            }

            for del_index in del_indexs.iter().rev() {
                let ups_heartbeat = ups_data.ups_heartbeats.remove(*del_index);
                log::info!(
                    "del name:{}, addr:{}, domain_index:{}, index:{}",
                    ups_data.ups_config.name,
                    ups_heartbeat.borrow().addr,
                    ups_heartbeat.borrow().domain_index,
                    ups_heartbeat.borrow().index,
                );
                ups_data
                    .ups_heartbeats_map
                    .remove(&ups_heartbeat.borrow().index);
                let _ = ups_heartbeat.borrow().shutdown_heartbeat_tx.send(());
            }

            let mut heartbeat_addrs = HashMap::new();
            for (_, ups_heartbeat) in ups_data.ups_heartbeats.iter().enumerate() {
                let key = format!(
                    "{}_{}",
                    ups_heartbeat.borrow().domain_index,
                    ups_heartbeat.borrow().addr
                );
                heartbeat_addrs.insert(key, true);
            }

            for (_, ups_dynamic_domain) in ups_data.ups_dynamic_domains.iter().enumerate() {
                for (_, addr) in ups_dynamic_domain.addrs.iter().enumerate() {
                    let key = format!("{}_{}", ups_dynamic_domain.index, addr);
                    if heartbeat_addrs.get(&key).is_some() {
                        continue;
                    }

                    let domaon_index = ups_dynamic_domain.index;
                    let ups_heartbeats_index = ups_data.ups_heartbeats_index;
                    let proxy_pass = &ups_dynamic_domain.proxy_pass;
                    let ups_heartbeat = UpstreamConfig::heartbeat(
                        ups_data.tcp_config_map.clone(),
                        ups_data.quic_config_map.clone(),
                        ups_data.endpoints_map.clone(),
                        ups_data.tunnel_clients.clone(),
                        domaon_index,
                        ups_heartbeats_index,
                        addr,
                        proxy_pass,
                    )?;
                    ups_data.ups_heartbeats.push(ups_heartbeat.clone());
                    ups_data
                        .ups_heartbeats_map
                        .insert(ups_heartbeats_index, ups_heartbeat.clone());
                    ups_data.ups_heartbeats_index += 1;

                    if ups_heartbeat.borrow().heartbeat.is_none() {
                        continue;
                    }
                    log::info!(
                        "add name:{}, addr:{}, domain_index:{}, index:{}",
                        ups_data.ups_config.name,
                        ups_heartbeat.borrow().addr,
                        ups_heartbeat.borrow().domain_index,
                        ups_heartbeat.borrow().index,
                    );

                    UpstreamHeartbeatServer::spawn_local(
                        self.work_group.worker().add(),
                        self.executors.clone(),
                        self.ups_data.clone(),
                        ups_heartbeat.borrow().index,
                    );
                }
            }

            if ups_data.is_disable_change {
                ups_data.is_disable_change = false;
                ups_data.ups_heartbeats_active.clear();
                for (_, v) in ups_data.ups_heartbeats.iter().enumerate() {
                    if !v.borrow().disable {
                        continue;
                    }
                    ups_data.ups_heartbeats_active.push(v.clone());
                }
            }

            *self.ups_data.borrow_mut() = Some(ups_data);
        }
    }
}
