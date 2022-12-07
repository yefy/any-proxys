use super::port::PortListen;
use super::port_stream;
use crate::config::config_toml;
#[cfg(feature = "anyproxy-ebpf")]
use crate::ebpf::ebpf_sockhash;
use crate::stream::server;
use crate::Executors;
use crate::Tunnels;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct PortServer {
    executors: Executors,
    tunnels: Tunnels,
    listen_shutdown_tx: broadcast::Sender<()>,
    common_config: config_toml::CommonConfig,
    listen_server: Rc<Box<dyn server::Server>>,
    key: String,
    port_config_listen_map: Rc<RefCell<HashMap<String, PortListen>>>,
    tmp_file_id: Rc<RefCell<u64>>,
    #[cfg(feature = "anyproxy-ebpf")]
    ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
}

impl PortServer {
    pub fn new(
        executors: Executors,
        tunnels: Tunnels,
        listen_shutdown_tx: broadcast::Sender<()>,
        common_config: config_toml::CommonConfig,
        listen_server: Rc<Box<dyn server::Server>>,
        key: String,
        port_config_listen_map: Rc<RefCell<HashMap<String, PortListen>>>,
        tmp_file_id: Rc<RefCell<u64>>,
        #[cfg(feature = "anyproxy-ebpf")] ebpf_add_sock_hash: Arc<ebpf_sockhash::AddSockHash>,
    ) -> Result<PortServer> {
        Ok(PortServer {
            executors,
            tunnels,
            listen_shutdown_tx,
            common_config,
            listen_server,
            key,
            port_config_listen_map,
            tmp_file_id,
            #[cfg(feature = "anyproxy-ebpf")]
            ebpf_add_sock_hash,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let shutdown_timeout = self.common_config.shutdown_timeout;
        let listen_server = self.listen_server.clone();
        let executors = self.executors.clone();
        let key = self.key.clone();
        #[cfg(feature = "anyproxy-ebpf")]
        let ebpf_add_sock_hash = self.ebpf_add_sock_hash.clone();

        let tmp_file_id = self.tmp_file_id.clone();
        let port_config_listen_map = self.port_config_listen_map.clone();
        let proto = listen_server.protocol7().to_protocol4();
        let listen_addr = listen_server
            .listen_addr()
            .map_err(|e| anyhow!("err:listen_server.listen_addr => e:{}", e))?;
        let (tunnel2_listen, tunnel2_publish) =
            self.tunnels.server2.listen(proto, &listen_addr).await;
        let (tunnel_listen, tunnel_publish) = self.tunnels.server.listen().await;
        server::listen(
            executors.clone(),
            tunnel_listen,
            tunnel2_listen,
            shutdown_timeout,
            listen_server,
            self.listen_shutdown_tx.clone(),
            move |protocol_name, stream, local_addr, remote_addr, ssl_domain, shutdown_tx| {
                let key = key.clone();
                let port_config_listen_map = port_config_listen_map.clone();
                let tunnel_publish = tunnel_publish.clone();
                let tunnel2_publish = tunnel2_publish.clone();
                let tmp_file_id = tmp_file_id.clone();
                #[cfg(feature = "anyproxy-ebpf")]
                let ebpf_add_sock_hash = ebpf_add_sock_hash.clone();

                let executors = executors.clone();
                async move {
                    let port_config_listen = {
                        let port_config_listen_map_borrow = port_config_listen_map.borrow_mut();
                        let port_listen = port_config_listen_map_borrow.get(&key);
                        if port_listen.is_none() {
                            log::error!(
                                "err:port_config_listen_map => key:{} invalid, group_version:{}",
                                key,
                                executors.group_version
                            );
                            return Ok(());
                        }
                        let port_listen = port_listen.unwrap();
                        let port_config_listen = port_listen.port_config_listen.clone();
                        port_config_listen
                    };

                    let port_stream = port_stream::PortStream::new(
                        executors,
                        tunnel_publish,
                        tunnel2_publish,
                        local_addr,
                        remote_addr,
                        ssl_domain,
                        shutdown_tx,
                        port_config_listen,
                        tmp_file_id,
                        #[cfg(feature = "anyproxy-ebpf")]
                        ebpf_add_sock_hash,
                    )
                    .map_err(|e| anyhow!("err:PortStream::new => e:{}", e))?;
                    port_stream
                        .start(protocol_name, stream)
                        .await
                        .map_err(|e| anyhow!("err:port_stream.start => e:{}", e))?;

                    Ok(())
                }
            },
        )
        .await
        .map_err(|e| anyhow!("err:server::listen => e:{}", e))?;
        Ok(())
    }
}
