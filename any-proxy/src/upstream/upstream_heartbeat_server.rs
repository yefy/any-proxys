use super::UpstreamData;
use crate::protopack;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::io::buf_reader::BufReader;
use any_base::io::buf_writer::BufWriter;
use any_base::stream_flow::{StreamFlowRead, StreamFlowWrite};
use any_base::typ::ShareRw;
use anyhow::anyhow;
use anyhow::Result;
use std::time::Instant;

pub struct UpstreamHeartbeatServer {
    executors: ExecutorsLocal,
    ups_data: ShareRw<UpstreamData>,
    ///在map中的索引
    index: usize,
}

impl UpstreamHeartbeatServer {
    pub fn spawn_local(executors: ExecutorsLocal, ups_data: ShareRw<UpstreamData>, index: usize) {
        executors._start(
            #[cfg(feature = "anyspawn-count")]
            None,
            move |executors| async move {
                let heartbeat_server = UpstreamHeartbeatServer::new(executors, ups_data, index)
                    .map_err(|e| anyhow!("err:PortServer::new => e:{}", e))?;
                heartbeat_server
                    .start()
                    .await
                    .map_err(|e| anyhow!("err:port_server.start => e:{}", e))?;
                Ok(())
            },
        );
    }
    pub fn new(
        executors: ExecutorsLocal,
        ups_data: ShareRw<UpstreamData>,
        index: usize,
    ) -> Result<UpstreamHeartbeatServer> {
        Ok(UpstreamHeartbeatServer {
            executors,
            ups_data,
            index,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let (ups_heartbeat, ups_config_name) = {
            let ups_data = self.ups_data.get_mut();

            let ups_heartbeat = ups_data.ups_heartbeats_map.get(&self.index).cloned();
            if ups_heartbeat.is_none() {
                return Err(anyhow!("err:ups_heartbeats_map => index:{}", self.index));
            }
            (ups_heartbeat.unwrap(), ups_data.ups_config.name.clone())
        };

        scopeguard::defer! {
            log::info!("stop upstream_heartbeat_server ups_name:[{}], addr:{}",
                ups_config_name,
                ups_heartbeat.get().addr
            );
        }

        log::debug!(target: "main",
            "start upstream_heartbeat_server ups_name:[{}], addr:{}",
            ups_config_name,
            ups_heartbeat.get().addr
        );

        let (interval, _timeout, fail, connect, mut shutdown_heartbeat_rx) = {
            let tmp_ups_heartbeat = ups_heartbeat.get();
            let heartbeat = tmp_ups_heartbeat.heartbeat.as_ref().unwrap();
            (
                heartbeat.interval,
                heartbeat.timeout,
                heartbeat.fail,
                ups_heartbeat.get().connect.clone(),
                ups_heartbeat.get().shutdown_heartbeat_tx.subscribe(),
            )
        };

        let mut shutdown_thread_rx = self.executors.context.shutdown_thread_tx.subscribe();
        let mut r: Option<BufReader<StreamFlowRead>> = None;
        let mut w: Option<BufWriter<StreamFlowWrite>> = None;

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
                if r.is_none() {
                    let connect_info = connect
                        .connect(None, None, Some(self.executors.context.run_time.clone()))
                        .await
                        .map_err(|e| anyhow!("err:connect => e:{}", e))?;

                    let (upstream_stream, upstream_connect_info) = connect_info;
                    log::debug!(target: "main",
                        "new upstream_heartbeat upstream_addr:{}, remote_addr:{}",
                        upstream_connect_info.domain,
                        upstream_connect_info.remote_addr
                    );
                    let (r_, w_) = upstream_stream.split();
                    let r_ = any_base::io::buf_reader::BufReader::new(r_);
                    let w_ = any_base::io::buf_writer::BufWriter::new(w_);
                    r = Some(r_);
                    w = Some(w_);
                }

                let heartbeat_time = Instant::now();
                protopack::anyproxy::write_pack(
                    &mut w.as_mut().unwrap(),
                    protopack::anyproxy::AnyproxyHeaderType::Heartbeat,
                    &protopack::anyproxy::AnyproxyHeartbeat {
                        version: protopack::anyproxy::ANYPROXY_VERSION.to_string(),
                    },
                )
                .await
                .map_err(|e| anyhow!("err:anyproxy::write_pack => e:{}", e))?;

                let heartbeat_r = protopack::anyproxy::read_heartbeat_r(&mut r.as_mut().unwrap())
                    .await
                    .map_err(|e| anyhow!("err:anyproxy::read_hello => e:{}", e))?;
                if heartbeat_r.is_none() {
                    return Err(anyhow!("err:connect.connect => e:heartbeat_r nil"));
                }

                {
                    let heartbeat_time_elapsed = heartbeat_time.elapsed().as_millis();
                    let mut ups_heartbeat = ups_heartbeat.get_mut();
                    ups_heartbeat.total_elapsed += heartbeat_time_elapsed;
                    ups_heartbeat.count_elapsed += 1;
                    ups_heartbeat.avg_elapsed =
                        ups_heartbeat.total_elapsed / ups_heartbeat.count_elapsed as u128;

                    {
                        self.ups_data.get_mut().is_sort_heartbeats_active = true;
                    }

                    log::debug!(target: "main",
                        "heartbeat name:{}, index:{}, domain_index:{}, index:{}, addr:{}",
                        ups_config_name,
                        self.index,
                        ups_heartbeat.domain_index,
                        ups_heartbeat.index,
                        ups_heartbeat.addr,
                    );
                }
                Ok(())
            }
            .await;
            match ret {
                Ok(_) => {
                    let mut ups_heartbeat = ups_heartbeat.get_mut();
                    if ups_heartbeat.disable {
                        self.ups_data.get_mut().is_heartbeat_change = true;
                        ups_heartbeat.curr_fail = 0;
                        ups_heartbeat.disable = false;
                        log::info!(
                            "heartbeat name:{}, index:{}, domain_index:{}, index:{}, open addr:{}",
                            ups_config_name,
                            self.index,
                            ups_heartbeat.domain_index,
                            ups_heartbeat.index,
                            ups_heartbeat.addr
                        );
                    }
                }
                Err(e) => {
                    r = None;
                    w = None;
                    let mut ups_heartbeat = ups_heartbeat.get_mut();
                    ups_heartbeat.curr_fail += 1;
                    ups_heartbeat.effective_weight =
                        ups_heartbeat.weight / ups_heartbeat.curr_fail as i64;
                    if ups_heartbeat.curr_fail > fail && !ups_heartbeat.disable {
                        self.ups_data.get_mut().is_heartbeat_change = true;
                        ups_heartbeat.disable = true;
                        log::info!(
                                "heartbeat name:{}, index:{}, domain_index:{}, index:{}, disable addr:{}",
                                ups_config_name,
                                self.index,
                                ups_heartbeat.domain_index,
                                ups_heartbeat.index,
                                ups_heartbeat.addr
                            );
                    }
                    log::error!("err:upstream_heartbeat_server.rs => e:{}", e);
                }
            }
        }
    }
}
