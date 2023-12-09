use super::peer_client::PeerClient;
use super::stream::Stream;
use super::stream_flow::StreamFlow;
use crate::peer_stream::PeerStreamKey;
use crate::protopack::TunnelHello;
use any_base::executor_local_spawn::Runtime;
use any_base::stream::StreamReadWriteTokio;
use any_base::stream_flow::StreamReadWriteFlow;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

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
    run_time: Arc<Box<dyn Runtime>>,
}

impl Listener {
    pub fn new(
        accept_rx: AcceptReceiverType,
        server: Server,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> Listener {
        Listener {
            accept_rx,
            server,
            run_time,
        }
    }

    pub async fn accept(&mut self) -> Result<(Stream, SocketAddr, SocketAddr, Option<String>)> {
        loop {
            let accept = self
                .server
                .accept(&mut self.accept_rx, self.run_time.clone())
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
    run_time: Arc<Box<dyn Runtime>>,
}

impl Publish {
    pub fn new(
        accept_tx: AcceptSenderType,
        server: Server,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> Publish {
        Publish {
            accept_tx,
            server,
            run_time,
        }
    }

    pub async fn push_peer_stream_tokio<RW: StreamReadWriteTokio + 'static>(
        &self,
        rw: RW,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        domain: Option<String>,
    ) -> Result<()> {
        use any_base::stream::Stream;
        let rw = Stream::new(rw);
        self.push_peer_stream(rw, local_addr, remote_addr, domain)
            .await
    }

    pub async fn push_peer_stream<RW: StreamReadWriteFlow + 'static>(
        &self,
        rw: RW,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        domain: Option<String>,
    ) -> Result<()> {
        self.server
            .create_accept_connect(
                StreamFlow::new(0, rw),
                local_addr,
                remote_addr,
                domain,
                self.accept_tx.clone(),
                self.run_time.clone(),
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
    context: ArcMutex<ServerContext>,
}

impl Server {
    pub fn new() -> Server {
        Server {
            context: ArcMutex::new(ServerContext::new()),
        }
    }

    pub async fn create_accept_connect(
        &self,
        stream: StreamFlow,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        domain: Option<String>,
        accept_tx: AcceptSenderType,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> Result<()> {
        PeerClient::do_create_peer_stream(
            false,
            "".to_string(),
            local_addr,
            remote_addr,
            domain,
            None,
            stream,
            Some(accept_tx),
            run_time,
        )
        .await
        .map_err(|e| anyhow!("er:do_create_peer_stream => e:{}", e))?;
        Ok(())
    }

    pub async fn accept(
        &self,
        accept_rx: &mut AcceptReceiverType,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> Result<(Stream, SocketAddr, SocketAddr, Option<String>)> {
        loop {
            let recv_msg = accept_rx
                .recv()
                .await
                .map_err(|e| anyhow!("er:to_server_rx => e:{}", e))?;
            let ret: Result<Option<(Stream, SocketAddr, SocketAddr, Option<String>)>> = async {
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
                let peer_client = { self.context.get_mut().get(&session_id) };
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

                    let ret = peer_client
                        .context
                        .hello_to_peer_stream(
                            None,
                            &peer_stream_key,
                            tunnel_hello.min_stream_cache_size,
                            Some(session_id),
                            Some(tunnel_hello.client_peer_stream_index),
                        )
                        .await
                        .map_err(|e| anyhow!("er:hello_to_peer_stream => e:{}", e));

                    let _e = match ret {
                        Err(e) => e,
                        Ok(ret) => match ret {
                            Some(_) => {
                                return Ok(None);
                            }
                            None => {
                                anyhow!("peer_client close")
                            }
                        },
                    };

                    #[cfg(feature = "anydebug")]
                    log::info!(
                        "server close 222 tunnel_hello :{:?}， _e: {}",
                        tunnel_hello,
                        _e
                    );

                    peer_stream_key.to_peer_stream_tx.close();
                    return Ok(None);
                }

                #[cfg(feature = "anydebug")]
                log::info!("server create tunnel_hello:{:?}", tunnel_hello);
                let (peer_client, stream, local_addr, remote_addr, domain) = {
                    let ret = PeerClient::create_stream_and_peer_client(
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
                        run_time.clone(),
                    )
                    .await
                    .map_err(|e| anyhow!("er:create_stream_and_peer_client => e:{}", e));

                    if let Err(_e) = ret {
                        #[cfg(feature = "anydebug")]
                        log::info!(
                            "server close 333 tunnel_hello :{:?}， _e: {}",
                            tunnel_hello,
                            _e
                        );

                        peer_stream_key.to_peer_stream_tx.close();
                        return Ok(None);
                    };
                    ret.unwrap()
                };

                {
                    self.context
                        .get_mut()
                        .insert(session_id.clone(), Some(peer_client.clone()));
                }

                Ok(Some((stream, local_addr, remote_addr, domain)))
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

    pub async fn listen(&self, run_time: Arc<Box<dyn Runtime>>) -> (Listener, Publish) {
        let (accept_tx, accept_rx) = async_channel::unbounded::<ServerRecv>();
        (
            Listener::new(accept_rx, self.clone(), run_time.clone()),
            Publish::new(accept_tx, self.clone(), run_time),
        )
    }
}
