use super::port::PortListen;
use super::port_stream;
use crate::stream::server;
use crate::stream::server::Listener;
use any_base::executor_local_spawn::{ExecutorLocalSpawn, ExecutorsLocal};
use any_base::module::module::Modules;
use any_base::typ::ShareRw;
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct PortServer {
    ms: Modules,
    executors: ExecutorsLocal,
    listen_shutdown_tx: broadcast::Sender<()>,
    listen_server: Arc<Box<dyn server::Server>>,
    key: String,
    port_config_listen_map: ShareRw<HashMap<String, PortListen>>,
}

impl PortServer {
    pub fn new(
        ms: Modules,
        executors: ExecutorsLocal,
        listen_shutdown_tx: broadcast::Sender<()>,
        listen_server: Arc<Box<dyn server::Server>>,
        key: String,
        port_config_listen_map: ShareRw<HashMap<String, PortListen>>,
    ) -> Result<PortServer> {
        Ok(PortServer {
            ms,
            executors,
            listen_shutdown_tx,
            listen_server,
            key,
            port_config_listen_map,
        })
    }

    pub async fn start(&self) -> Result<()> {
        log::trace!("port server start");
        use crate::config::tunnel2_core;
        use crate::config::tunnel_core;
        let tunnel_core_conf = tunnel_core::main_conf_mut(&self.ms).await;
        let tunnel2_core_conf = tunnel2_core::main_conf_mut(&self.ms).await;

        let listen_server = self.listen_server.clone();
        let listen_addr = listen_server
            .listen_addr()
            .map_err(|e| anyhow!("err:listen_server.listen_addr => e:{}", e))?;
        let proto = listen_server.protocol7().to_protocol4();
        let (tunnel2_listen, tunnel2_publish) = if tunnel2_core_conf.server().is_some() {
            let (tunnel2_listen, tunnel2_publish) = tunnel2_core_conf
                .server()
                .unwrap()
                .listen(proto, &listen_addr)
                .await;
            (Some(tunnel2_listen), Some(tunnel2_publish))
        } else {
            (None, None)
        };
        let (tunnel_listen, tunnel_publish) = if tunnel_core_conf.server().is_some() {
            let (tunnel_listen, tunnel_publish) = tunnel_core_conf
                .server()
                .unwrap()
                .listen(self.executors.context.run_time.clone())
                .await;
            (Some(tunnel_listen), Some(tunnel_publish))
        } else {
            (None, None)
        };

        let listens = server::get_listens(tunnel_listen, tunnel2_listen, listen_server).await?;
        let executor = ExecutorLocalSpawn::new(self.executors.clone());
        for listen in listens.into_iter() {
            self.do_start(
                executor.clone(),
                listen,
                tunnel_publish.clone(),
                tunnel2_publish.clone(),
            )
            .await?;
        }

        let mut shutdown_thread_tx = self.executors.context.shutdown_thread_tx.subscribe();
        tokio::select! {
            biased;
            _ret = executor.wait("port_server wait") => {
                _ret?;
            }
            is_fast_shutdown = shutdown_thread_tx.recv() => {
                let is_fast_shutdown = is_fast_shutdown?;
                executor.send("port_server send",is_fast_shutdown);
            }
            else => {
                return Err(anyhow!("err:accept"));
            }
        }

        executor.wait("port_server wait").await?;
        Ok(())
    }

    pub async fn do_start(
        &self,
        mut executor: ExecutorLocalSpawn,
        listen: Box<dyn Listener>,
        tunnel_publish: Option<tunnel_server::Publish>,
        tunnel2_publish: Option<tunnel2_server::Publish>,
    ) -> Result<()> {
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&self.ms).await;

        let shutdown_timeout = common_core_conf.shutdown_timeout;
        let listen_server = self.listen_server.clone();
        let key = self.key.clone();
        let port_config_listen_map = self.port_config_listen_map.clone();
        let listen_shutdown_tx = self.listen_shutdown_tx.clone();
        let ms = self.ms.clone();
        executor._start(
            #[cfg(feature = "anyspawn-count")]
            None,
            move |executors| async move {
                server::listen(
                    #[cfg(feature = "anyspawn-count")]
                    format!("{}:{}", file!(), line!()),
                    executors,
                    shutdown_timeout,
                    listen_shutdown_tx,
                    listen_server,
                    listen,
                    move |stream, server_stream_info, executors| async move {
                        let port_config_listen = {
                            let port_config_listen_map = port_config_listen_map.get();
                            let port_listen = port_config_listen_map.get(&key);
                            if port_listen.is_none() {
                                log::error!(
                                "err:port_config_listen_map => key:{} invalid, group_version:{}",
                                key,
                                executors.context.group_version
                            );
                                return Ok(());
                            }
                            let port_listen = port_listen.unwrap();
                            let port_config_listen = port_listen.port_config_listen.clone();
                            port_config_listen
                        };

                        let port_stream = port_stream::PortStream::new(
                            ms,
                            executors,
                            server_stream_info,
                            tunnel_publish,
                            tunnel2_publish,
                            port_config_listen,
                            stream,
                        )
                        .map_err(|e| anyhow!("err:PortStream::new => e:{}", e))?;
                        port_stream
                            .start()
                            .await
                            .map_err(|e| anyhow!("err:port_stream.start => e:{}", e))?;
                        Ok(())
                    },
                )
                .await
                .map_err(|e| anyhow!("err:server::listen => e:{}", e))
            },
        );
        Ok(())
    }
}
