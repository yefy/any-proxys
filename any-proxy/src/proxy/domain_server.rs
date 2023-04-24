use super::domain::DomainListen;
use super::domain_stream;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::any_ebpf;
use crate::proxy::domain_context::DomainContext;
use crate::stream::server;
use crate::stream::server::Listener;
use crate::stream::server::Server;
use crate::Tunnels;
use any_base::executor_local_spawn::{ExecutorLocalSpawn, ExecutorsLocal};
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct DomainServer {
    executors: ExecutorsLocal,
    tunnels: Tunnels,
    listen_shutdown_tx: broadcast::Sender<()>,
    common_config: config_toml::CommonConfig,
    listen_server: Rc<Box<dyn Server>>,
    key: String,
    domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
    session_id: Arc<AtomicU64>,
    domain_context: Rc<DomainContext>,
}

impl DomainServer {
    pub fn new(
        executors: ExecutorsLocal,
        tunnels: Tunnels,
        listen_shutdown_tx: broadcast::Sender<()>,
        common_config: config_toml::CommonConfig,
        listen_server: Rc<Box<dyn Server>>,
        key: String,
        domain_config_listen_map: Rc<RefCell<HashMap<String, DomainListen>>>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<any_ebpf::AddSockHash>,
        session_id: Arc<AtomicU64>,
        domain_context: Rc<DomainContext>,
    ) -> Result<DomainServer> {
        Ok(DomainServer {
            executors,
            tunnels,
            listen_shutdown_tx,
            common_config,
            listen_server,
            key,
            domain_config_listen_map,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
            session_id,
            domain_context,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let listen_server = self.listen_server.clone();
        let listen_addr = listen_server
            .listen_addr()
            .map_err(|e| anyhow!("err:listen_server.listen_addr => e:{}", e))?;
        let proto = listen_server.protocol7().to_protocol4();
        let (tunnel2_listen, tunnel2_publish) =
            self.tunnels.server2.listen(proto, &listen_addr).await;
        let (tunnel_listen, tunnel_publish) = self.tunnels.server.listen().await;
        let listens = server::get_listens(tunnel_listen, tunnel2_listen, listen_server).await?;
        let executor_local_spawn = ExecutorLocalSpawn::new(self.executors.clone());
        for listen in listens.into_iter() {
            self.do_start(
                executor_local_spawn.clone(),
                listen,
                tunnel_publish.clone(),
                tunnel2_publish.clone(),
            )
            .await?;
        }

        let mut shutdown_thread_tx = self.executors.shutdown_thread_tx.subscribe();
        tokio::select! {
            biased;
            _ret = executor_local_spawn.wait("domain_server wait") => {
                _ret?;
            }
            is_fast_shutdown = shutdown_thread_tx.recv() => {
                let is_fast_shutdown = is_fast_shutdown?;
                executor_local_spawn.send("domain_server send", is_fast_shutdown);
            }
            else => {
                return Err(anyhow!("err:accept"));
            }
        }

        executor_local_spawn.wait("domain_server wait").await?;
        Ok(())
    }

    pub async fn do_start(
        &self,
        mut executor_local_spawn: ExecutorLocalSpawn,
        listen: Box<dyn Listener>,
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
    ) -> Result<()> {
        let shutdown_timeout = self.common_config.shutdown_timeout;
        let listen_server = self.listen_server.clone();
        let key = self.key.clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = self.ebpf_add_sock_hash.clone();
        let session_id = self.session_id.clone();
        let tmp_file_id = self.tmp_file_id.clone();
        let domain_config_listen_map = self.domain_config_listen_map.clone();
        let listen_shutdown_tx = self.listen_shutdown_tx.clone();
        let domain_context = self.domain_context.clone();
        executor_local_spawn._start(
            #[cfg(feature = "anyspawn-count")]
            format!("{}:{}", file!(), line!()),
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
                        let domain_config_listen = {
                            let domain_config_listen_map_borrow =
                                domain_config_listen_map.borrow_mut();
                            let domain_listen = domain_config_listen_map_borrow.get(&key);
                            if domain_listen.is_none() {
                                log::error!(
                                "err:domain_config_listen_map => key:{} invalid, group_version:{}",
                                key,
                                executors.group_version
                            );
                                return Ok(());
                            }
                            let domain_listen = domain_listen.unwrap();
                            let domain_config_listen = domain_listen.domain_config_listen.clone();
                            domain_config_listen
                        };

                        let domain_stream = domain_stream::DomainStream::new(
                            executors,
                            server_stream_info,
                            tunnel_publish,
                            tunnel2_publish,
                            domain_config_listen,
                            tmp_file_id,
                            #[cfg(feature = "anyproxy-ebpf")]
                            ebpf_add_sock_hash,
                            session_id,
                            domain_context,
                        )
                        .map_err(|e| anyhow!("err:DomainStream::new => e:{}", e))?;
                        domain_stream
                            .start(stream)
                            .await
                            .map_err(|e| anyhow!("err:domain_stream.start => e:{}", e))?;
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
