use super::peer_client::PeerClient;
use super::stream::Stream;
use super::stream_flow::StreamFlow;
use crate::peer_stream::PeerStreamKey;
use crate::protopack::TunnelHello;
use anyhow::anyhow;
use anyhow::Result;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};

pub type AcceptSenderType = async_channel::Sender<ServerRecv>;
pub type AcceptReceiverType = async_channel::Receiver<ServerRecv>;

pub struct ServerRecvHello {
    pub peer_stream_key: Arc<PeerStreamKey>,
    pub tunnel_hello: TunnelHello,
}

pub enum ServerRecv {
    ServerRecvHello(ServerRecvHello),
}

pub struct Listener {
    accept_rx: AcceptReceiverType,
    server: Server,
}

impl Listener {
    pub fn new(accept_rx: AcceptReceiverType, server: Server) -> Listener {
        Listener { accept_rx, server }
    }

    pub async fn accept(&mut self) -> Result<(Stream, SocketAddr, SocketAddr)> {
        loop {
            let accept = self
                .server
                .accept(&mut self.accept_rx)
                .await
                .map_err(|e| anyhow!("err:any-tunnel accept => e:{}", e))?;
            return Ok(accept);
        }
    }
}

#[derive(Clone)]
pub struct Publish {
    accept_tx: AcceptSenderType,
    server: Server,
}

impl Publish {
    pub fn new(accept_tx: AcceptSenderType, server: Server) -> Publish {
        Publish { accept_tx, server }
    }

    pub async fn push_peer_stream(
        &self,
        r: Box<dyn AsyncRead + Send + Unpin>,
        w: Box<dyn AsyncWrite + Send + Unpin>,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<()> {
        self.server
            .create_accept_connect(
                StreamFlow::new(0, r, w),
                local_addr,
                remote_addr,
                self.accept_tx.clone(),
            )
            .await
            .map_err(|e| anyhow!("er:create_connect => e:{}", e))
    }
}

pub struct ServerContext {
    peer_client_map: HashMap<String, Option<Arc<PeerClient>>>,
}

impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            peer_client_map: HashMap::new(),
        }
    }

    pub fn get(&mut self, session_id: &str) -> Option<Option<Arc<PeerClient>>> {
        self.peer_client_map.get(session_id).cloned()
    }

    pub fn insert(&mut self, session_id: String, peer_client: Option<Arc<PeerClient>>) {
        self.peer_client_map.insert(session_id, peer_client);
    }

    pub fn delete(&mut self, session_id: String) {
        self.peer_client_map.insert(session_id, None);
    }
}

#[derive(Clone)]
pub struct Server {
    context: Arc<Mutex<ServerContext>>,
}

impl Server {
    pub fn new() -> Server {
        Server {
            context: Arc::new(Mutex::new(ServerContext::new())),
        }
    }

    pub async fn create_accept_connect(
        &self,
        stream: StreamFlow,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        accept_tx: AcceptSenderType,
    ) -> Result<()> {
        PeerClient::do_create_peer_stream(
            false,
            "".to_string(),
            local_addr,
            remote_addr,
            None,
            stream,
            Some(accept_tx),
        )
        .await
        .map_err(|e| anyhow!("er:do_create_peer_stream => e:{}", e))?;
        Ok(())
    }

    pub async fn accept(
        &self,
        accept_rx: &mut AcceptReceiverType,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        loop {
            let recv_msg = accept_rx
                .recv()
                .await
                .map_err(|e| anyhow!("er:to_server_rx => e:{}", e))?;
            let ret: Result<Option<(Stream, SocketAddr, SocketAddr)>> = async {
                let recv_msg = match recv_msg {
                    ServerRecv::ServerRecvHello(recv_msg) => recv_msg,
                };

                let ServerRecvHello {
                    peer_stream_key,
                    tunnel_hello,
                } = recv_msg;

                #[cfg(feature = "anydebug")]
                log::info!("accept flag:server, tunnel_hello:{:?}", tunnel_hello);

                log::debug!("server tunnel_hello:{:?}", tunnel_hello);
                let session_id = tunnel_hello.session_id.clone();
                let peer_client = { self.context.lock().unwrap().get(&session_id) };
                if peer_client.is_some() {
                    let peer_client = peer_client.unwrap();
                    if peer_client.is_none() {
                        peer_stream_key.to_peer_stream_tx.close();
                        #[cfg(feature = "anydebug")]
                        log::info!("server close tunnel_hello :{:?}", tunnel_hello);
                        //return Err(anyhow!("accept session_id closed:{}", session_id));
                        return Ok(None);
                    }
                    let peer_client = peer_client.unwrap();

                    #[cfg(feature = "anydebug")]
                    log::info!("server add tunnel_hello:{:?}", tunnel_hello);
                    if let Err(_e) = peer_client
                        .hello_to_peer_stream(
                            None,
                            &peer_stream_key,
                            tunnel_hello.min_stream_cache_size,
                            Some(session_id),
                            Some(tunnel_hello.client_peer_stream_index),
                        )
                        .await
                        .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e))
                    {
                        #[cfg(feature = "anydebug")]
                        log::info!(
                            "server close 222 tunnel_hello :{:?}， _e: {}",
                            tunnel_hello,
                            _e
                        );

                        peer_stream_key.to_peer_stream_tx.close();
                        return Ok(None);
                    }
                    return Ok(None);
                }

                #[cfg(feature = "anydebug")]
                log::info!("server create tunnel_hello:{:?}", tunnel_hello);
                let (peer_client, stream, local_addr, remote_addr) =
                    PeerClient::create_stream_and_peer_client(
                        false,
                        10,
                        None,
                        None,
                        None,
                        Some(&peer_stream_key),
                        tunnel_hello.min_stream_cache_size,
                        None,
                        Some(session_id.clone()),
                        Some(self.context.clone()),
                        tunnel_hello.channel_size,
                        Some(tunnel_hello.client_peer_stream_index),
                    )
                    .await
                    .map_err(|e| anyhow!("er:create_stream_and_peer_client => e:{}", e))?;

                {
                    self.context
                        .lock()
                        .unwrap()
                        .insert(session_id.clone(), Some(peer_client.clone()));
                }

                Ok(Some((stream, local_addr, remote_addr)))
            }
            .await;

            if let Err(e) = ret {
                log::error!("{}", e);
                continue;
            }
            let ret = ret.unwrap();
            if ret.is_none() {
                continue;
            }
            return Ok(ret.unwrap());
        }
    }

    pub async fn listen(&self) -> (Listener, Publish) {
        let (accept_tx, accept_rx) = async_channel::unbounded::<ServerRecv>();
        (
            Listener::new(accept_rx, self.clone()),
            Publish::new(accept_tx, self.clone()),
        )
    }
}
