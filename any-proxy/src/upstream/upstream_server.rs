use super::upstream_dynamic_domain_server::UpstreamDynamicDomainServer;
use super::upstream_heartbeat_server::UpstreamHeartbeatServer;
use super::UpstreamData;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::module::module::Modules;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;

pub struct UpstreamServer {
    executors: ExecutorsLocal,
    ups_data: ArcMutex<UpstreamData>,
    ms: Modules,
}

impl UpstreamServer {
    pub fn new(
        executors: ExecutorsLocal,
        ups_data: ArcMutex<UpstreamData>,
        ms: Modules,
    ) -> Result<UpstreamServer> {
        Ok(UpstreamServer {
            executors,
            ups_data,
            ms,
        })
    }

    pub async fn start(&self) -> Result<()> {
        self.run_ups_dynamic_domains().await;
        self.run_ups_heartbeats().await;
        self.run().await
    }

    pub async fn run_ups_dynamic_domains(&self) {
        let ups_data = self.ups_data.get_mut();
        log::debug!(
            "start run_ups_dynamic_domains ups_name:[{}]",
            ups_data.ups_config.name
        );

        for (index, ups_dynamic_domain) in ups_data.ups_dynamic_domains.iter().enumerate() {
            if ups_dynamic_domain.get().dynamic_domain.is_none() {
                continue;
            }
            UpstreamDynamicDomainServer::spawn_local(
                self.executors.clone(),
                self.ups_data.clone(),
                index,
            );
        }
    }

    pub async fn run_ups_heartbeats(&self) {
        let ups_data = self.ups_data.get_mut();
        log::debug!(
            "start run_ups_heartbeats ups_name:[{}]",
            ups_data.ups_config.name
        );
        for (_, ups_heartbeat) in ups_data.ups_heartbeats.iter().enumerate() {
            if ups_heartbeat.get().heartbeat.is_none() {
                continue;
            }
            UpstreamHeartbeatServer::spawn_local(
                self.executors.clone(),
                self.ups_data.clone(),
                ups_heartbeat.get().index,
            );
        }
    }

    pub async fn run(&self) -> Result<()> {
        {
            let ups_data = self.ups_data.get_mut();
            log::debug!("start ups_name:[{}]", ups_data.ups_config.name);
        }

        let mut shutdown_thread_rx = self.executors.context.shutdown_thread_tx.subscribe();
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

            if self.ups_data.get().is_dynamic_domain_change {
                self.ups_data.get_mut().is_dynamic_domain_change = false;
                self.ups_data.get_mut().is_heartbeat_disable = true;
                let mut domain_addrs = HashMap::new();
                {
                    for (_, ups_dynamic_domain) in
                        self.ups_data.get().ups_dynamic_domains.iter().enumerate()
                    {
                        let ups_dynamic_domain = ups_dynamic_domain.get();
                        for (_, addr) in ups_dynamic_domain.addrs.iter().enumerate() {
                            let key = format!("{}_{}", ups_dynamic_domain.index, addr);
                            domain_addrs.insert(key, true);
                        }
                    }
                }
                let mut del_indexs = Vec::new();
                {
                    for (index, ups_heartbeat) in
                        self.ups_data.get().ups_heartbeats.iter().enumerate()
                    {
                        let key = format!(
                            "{}_{}",
                            ups_heartbeat.get().domain_index,
                            ups_heartbeat.get().addr
                        );
                        if domain_addrs.get(&key).is_none() {
                            del_indexs.push(index);
                        }
                    }
                }

                for del_index in del_indexs.iter().rev() {
                    let ups_heartbeat = self.ups_data.get_mut().ups_heartbeats.remove(*del_index);
                    {
                        let ups_heartbeat = ups_heartbeat.get();
                        log::info!(
                            "del name:{}, addr:{}, domain_index:{}, index:{}",
                            self.ups_data.get().ups_config.name,
                            ups_heartbeat.addr,
                            ups_heartbeat.domain_index,
                            ups_heartbeat.index,
                        );
                    }
                    self.ups_data
                        .get_mut()
                        .ups_heartbeats_map
                        .remove(&ups_heartbeat.get().index);
                    let _ = ups_heartbeat.get().shutdown_heartbeat_tx.send(());
                }

                let mut heartbeat_addrs = HashMap::new();
                for (_, ups_heartbeat) in self.ups_data.get().ups_heartbeats.iter().enumerate() {
                    let key = format!(
                        "{}_{}",
                        ups_heartbeat.get().domain_index,
                        ups_heartbeat.get().addr
                    );
                    heartbeat_addrs.insert(key, true);
                }

                let ups_dynamic_domains = self.ups_data.get().ups_dynamic_domains.clone();
                for (_, ups_dynamic_domain) in ups_dynamic_domains.iter().enumerate() {
                    let addrs = ups_dynamic_domain.get().addrs.clone();
                    for (_, addr) in addrs.iter().enumerate() {
                        let key = format!("{}_{}", ups_dynamic_domain.get().index, addr);
                        if heartbeat_addrs.get(&key).is_some() {
                            continue;
                        }

                        let ms = self.ms.clone();
                        let domain_index = ups_dynamic_domain.get().index;
                        let ups_heartbeats_index = self.ups_data.get().ups_heartbeats_index;
                        let heartbeat = ups_dynamic_domain.get().heartbeat.clone();
                        let host = ups_dynamic_domain.get().host.clone();
                        let ups_heartbeat = ups_dynamic_domain.get().ups_heartbeat.clone();
                        let is_weight = ups_dynamic_domain.get().is_weight.clone();
                        let ups_heartbeat = heartbeat
                            .heartbeat(
                                ms,
                                domain_index,
                                ups_heartbeats_index,
                                addr,
                                host,
                                ups_heartbeat,
                                is_weight,
                            )
                            .await?;

                        self.ups_data
                            .get_mut()
                            .ups_heartbeats
                            .push(ups_heartbeat.clone());
                        self.ups_data
                            .get_mut()
                            .ups_heartbeats_map
                            .insert(ups_heartbeats_index, ups_heartbeat.clone());
                        self.ups_data.get_mut().ups_heartbeats_index += 1;

                        if ups_heartbeat.get().heartbeat.is_none() {
                            continue;
                        }
                        {
                            let ups_heartbeat = ups_heartbeat.get();
                            log::info!(
                                "add name:{}, addr:{}, domain_index:{}, index:{}",
                                self.ups_data.get().ups_config.name,
                                ups_heartbeat.addr,
                                ups_heartbeat.domain_index,
                                ups_heartbeat.index,
                            );
                        }

                        UpstreamHeartbeatServer::spawn_local(
                            self.executors.clone(),
                            self.ups_data.clone(),
                            ups_heartbeat.get().index,
                        );
                    }
                }
            }

            let ups_data = &mut *self.ups_data.get_mut();
            if ups_data.is_heartbeat_disable {
                ups_data.is_heartbeat_disable = false;
                ups_data.is_sort_heartbeats_active = true;
                ups_data.ups_heartbeats_active.clear();
                for (_, v) in ups_data.ups_heartbeats.iter().enumerate() {
                    if !v.get().disable {
                        continue;
                    }
                    ups_data.ups_heartbeats_active.push(v.clone());
                }
            }

            use crate::config::upstream_core_plugin;
            if ups_data.is_sort_heartbeats_active
                && ups_data.ups_config.balancer.as_str() == upstream_core_plugin::FAIR
            {
                ups_data.is_sort_heartbeats_active = false;
                ups_data.ups_heartbeats_active.sort_by(|a, b| {
                    a.get()
                        .avg_elapsed
                        .partial_cmp(&b.get().avg_elapsed)
                        .unwrap()
                });
            }
        }
    }
}
