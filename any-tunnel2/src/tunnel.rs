use super::peer_client::PeerClient;
use super::peer_client::PeerClientSender;
use super::peer_stream_connect::PeerStreamConnect;
use super::server::AcceptSenderType;
use any_base::typ::ArcMutexTokio;
use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Tunnel {
    tx: mpsc::Sender<PeerClient>,
    peer_client_sender_map: ArcMutexTokio<HashMap<String, HashMap<String, PeerClientSender>>>,
}

impl Tunnel {
    pub async fn start() -> Tunnel {
        let (tx, mut rx) = mpsc::channel::<PeerClient>(10);
        let async_ = async move {
            log::info!("tunnel2 start");
            let (shutdown_tx, _) = broadcast::channel::<()>(100);
            loop {
                let peer_client = rx.recv().await;
                if peer_client.is_none() {
                    let _ = shutdown_tx.send(());
                    log::info!("tunnel2 stop");
                    return;
                }
                let mut peer_client = peer_client.unwrap();
                let mut shutdown_rx = shutdown_tx.subscribe();
                let async_ = Box::pin(async move {
                    tokio::select! {
                        biased;
                        ret = peer_client.start_cmd() => {
                            ret?;
                            return Ok(());
                        }
                        _ = shutdown_rx.recv() => {
                            return Ok(());
                        }
                        else => {
                            return Err(anyhow!("err:start_cmd"))?;
                        }
                    }
                });

                if cfg!(feature = "anyruntime-tokio-spawn-local") {
                    any_base::executor_local_spawn::_start_and_free(
                        move || async move { async_.await },
                    );
                } else {
                    any_base::executor_spawn::_start_and_free(move || async move { async_.await });
                }
            }
        };
        if cfg!(feature = "anyruntime-tokio-spawn-local") {
            any_base::executor_local_spawn::_start_and_free(move || async move {
                async_.await;
                Ok(())
            });
        } else {
            any_base::executor_spawn::_start_and_free(move || async move {
                async_.await;
                Ok(())
            });
        }
        Tunnel {
            tx,
            peer_client_sender_map: ArcMutexTokio::new(HashMap::new()),
        }
    }

    pub async fn start_thread(flag: String, thread_num: usize) -> Tunnel {
        let (tx, mut rx) = mpsc::channel::<PeerClient>(10);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(thread_num)
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    log::info!("tunnel2 start_thread {}", flag);
                    let (shutdown_tx, _) = broadcast::channel::<()>(100);
                    loop {
                        let peer_client = rx.recv().await;
                        if peer_client.is_none() {
                            let _ = shutdown_tx.send(());
                            log::info!("tunnel2 stop_thread");
                            return;
                        }
                        let mut peer_client = peer_client.unwrap();
                        let mut shutdown_rx = shutdown_tx.subscribe();
                        let async_ = Box::pin(async move {
                            tokio::select! {
                                biased;
                                ret = peer_client.start_cmd() => {
                                    ret?;
                                    return Ok(());
                                }
                                _ = shutdown_rx.recv() => {
                                    return Ok(());
                                }
                                else => {
                                    return Err(anyhow!("err:start_cmd"))?;
                                }
                            }
                        });

                        if cfg!(feature = "anyruntime-tokio-spawn-local") {
                            any_base::executor_local_spawn::_start_and_free(move || async move {
                                async_.await
                            });
                        } else {
                            any_base::executor_spawn::_start_and_free(move || async move {
                                async_.await
                            });
                        }
                    }
                });
        });
        Tunnel {
            tx,
            peer_client_sender_map: ArcMutexTokio::new(HashMap::new()),
        }
    }

    pub async fn get_peer_client(
        &self,
        tunnel_key: &String,
        register_id: &String,
    ) -> Option<PeerClientSender> {
        let peer_client_sender_map = self.peer_client_sender_map.get().await;
        let peer_client_sender_map = peer_client_sender_map.get(tunnel_key);
        if peer_client_sender_map.is_none() {
            return None;
        }
        let peer_client_sender_map = peer_client_sender_map.unwrap();
        peer_client_sender_map.get(register_id).cloned()
    }

    pub async fn insert_peer_client(
        &self,
        tunnel_key: String,
        register_id: String,
        session_id: String,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        peer_stream_max_len: Arc<AtomicUsize>,
        accept_tx: Option<AcceptSenderType>,
        peer_stream_connect: Option<(Arc<Box<dyn PeerStreamConnect>>, SocketAddr)>,
    ) -> Result<PeerClientSender> {
        let mut peer_client_sender_map = self.peer_client_sender_map.get_mut().await;
        let peer_client_sender = peer_client_sender_map.get_mut(&tunnel_key);
        let peer_client_sender_map = if peer_client_sender.is_none() {
            peer_client_sender_map.insert(tunnel_key.clone(), HashMap::new());
            peer_client_sender_map.get_mut(&tunnel_key).unwrap()
        } else {
            peer_client_sender.unwrap()
        };

        let peer_client_sender = match peer_client_sender_map.get(&register_id).cloned() {
            Some(peer_client_sender) => peer_client_sender,
            None => {
                let peer_client = PeerClient::new(
                    session_id,
                    peer_stream_max_len,
                    local_addr,
                    remote_addr,
                    accept_tx,
                    peer_stream_connect,
                );

                let peer_client_sender = peer_client.get_peer_client_sender();
                peer_client_sender_map.insert(register_id, peer_client_sender.clone());
                self.tx
                    .send(peer_client)
                    .await
                    .map_err(|_| anyhow!("err:insert_peer_client"))?;
                peer_client_sender
            }
        };
        Ok(peer_client_sender)
    }
}
