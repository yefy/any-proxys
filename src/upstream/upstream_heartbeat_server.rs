use super::UpstreamData;
use crate::io::buf_reader::BufReader;
use crate::io::buf_writer::BufWriter;
use crate::protopack;
use crate::Executors;
use anyhow::anyhow;
use anyhow::Result;
use awaitgroup::WorkerInner;
use futures_util::task::LocalSpawnExt;
use std::cell::RefCell;
use std::rc::Rc;

pub struct UpstreamHeartbeatServer {
    executors: Executors,
    ups_data: Rc<RefCell<Option<UpstreamData>>>,
    index: usize,
}

impl UpstreamHeartbeatServer {
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
                    let heartbeat_server =
                        UpstreamHeartbeatServer::new(executors.clone(), ups_data, index)
                            .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                    heartbeat_server
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
    ) -> Result<UpstreamHeartbeatServer> {
        Ok(UpstreamHeartbeatServer {
            executors: executors,
            ups_data,
            index,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let ups_heartbeat = self
            .ups_data
            .borrow()
            .as_ref()
            .unwrap()
            .ups_heartbeats_map
            .get(&self.index)
            .cloned();
        if ups_heartbeat.is_none() {
            return Err(anyhow!("err:ups_heartbeats_map => index:{}", self.index));
        }
        let ups_heartbeat = ups_heartbeat.unwrap();

        scopeguard::defer! {
            log::info!("stop upstream_heartbeat_server ups_name:[{}], addr:{}",
        self.ups_data.borrow().as_ref().unwrap().ups_config.name,
                ups_heartbeat.borrow().addr);
        }
        log::info!(
            "start upstream_heartbeat_server ups_name:[{}], addr:{}",
            self.ups_data.borrow().as_ref().unwrap().ups_config.name,
            ups_heartbeat.borrow().addr
        );
        let (interval, _timeout, fail, connect, mut shutdown_heartbeat_rx) = {
            let tmp_ups_heartbeat = ups_heartbeat.borrow();
            let heartbeat = tmp_ups_heartbeat.heartbeat.as_ref().unwrap();
            (
                heartbeat.interval,
                heartbeat.timeout,
                heartbeat.fail,
                ups_heartbeat.borrow().connect.clone(),
                ups_heartbeat.borrow().shutdown_heartbeat_tx.subscribe(),
            )
        };

        loop {
            let connect_info = connect
                .connect(&mut None)
                .await
                .map_err(|e| anyhow!("err:connect => e:{}", e));
            if connect_info.is_err() {
                continue;
            }

            let (
                _upstream_protocol_name,
                upstream_stream,
                _upstream_addr,
                _upstream_elapsed,
                _local_addr,
                _remote_addr,
            ) = connect_info.unwrap();
            let mut w = BufWriter::new(&upstream_stream);
            let mut r = BufReader::new(&upstream_stream);

            let mut shutdown_thread_rx = self.executors.shutdown_thread_tx.subscribe();
            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval as u64)) => {
                    }
                     _ =  shutdown_heartbeat_rx.recv() => {
                        return Ok(());
                    }
                    _ =  shutdown_thread_rx.recv() => {
                        return Ok(());
                    }
                    else => {
                        return Err(anyhow!("err:select"));
                    }
                }

                let ret: Result<()> = async {
                    protopack::anyproxy::write_pack(
                        &mut w,
                        protopack::anyproxy::AnyproxyHeaderType::Heartbeat,
                        &protopack::anyproxy::AnyproxyHeartbeat {
                            version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                        },
                    )
                    .await
                    .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;

                    let heartbeat_r = protopack::anyproxy::read_heartbeat_r(&mut r)
                        .await
                        .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
                    if heartbeat_r.is_none() {
                        return Err(anyhow!("err:connect.connect => e:heartbeat_r nil"));
                    }

                    log::debug!(
                        "heartbeat name:{}, index:{}, domain_index:{}, index:{}, addr:{}",
                        self.ups_data.borrow().as_ref().unwrap().ups_config.name,
                        self.index,
                        ups_heartbeat.borrow().domain_index,
                        ups_heartbeat.borrow().index,
                        ups_heartbeat.borrow().addr,
                    );
                    Ok(())
                }
                .await;
                match ret {
                    Ok(_) => {
                        if ups_heartbeat.borrow().disable {
                            self.ups_data
                                .borrow_mut()
                                .as_mut()
                                .unwrap()
                                .is_disable_change = true;
                            ups_heartbeat.borrow_mut().curr_fail = 0;
                            ups_heartbeat.borrow_mut().disable = false;
                            log::info!(
                                "heartbeat name:{}, index:{}, domain_index:{}, index:{}, open addr:{}",
                                self.ups_data.borrow().as_ref().unwrap().ups_config.name,
                                self.index,
                                ups_heartbeat.borrow().domain_index,
                                ups_heartbeat.borrow().index,
                                ups_heartbeat.borrow().addr
                            );
                        }
                    }
                    Err(e) => {
                        ups_heartbeat.borrow_mut().curr_fail += 1;
                        if ups_heartbeat.borrow_mut().curr_fail > fail {
                            self.ups_data
                                .borrow_mut()
                                .as_mut()
                                .unwrap()
                                .is_disable_change = true;
                            ups_heartbeat.borrow_mut().disable = true;
                            log::info!(
                                "heartbeat name:{}, index:{}, domain_index:{}, index:{}, disable addr:{}",
                                self.ups_data.borrow().as_ref().unwrap().ups_config.name,
                                self.index,
                                ups_heartbeat.borrow().domain_index,
                                ups_heartbeat.borrow().index,
                                ups_heartbeat.borrow().addr
                            );
                        }
                        log::error!("err:upstream_heartbeat_server.rs => e:{}", e);
                    }
                }
            }
        }
    }
}
