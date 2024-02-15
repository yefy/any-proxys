use super::peer_client::PeerClient;
use super::protopack;
use super::stream::Stream;
use super::stream_flow::StreamFlow;
use super::tunnel::Tunnel;
use super::Protocol4;
use crate::peer_client::PeerClientSender;
use any_base::stream_flow::StreamReadWriteFlow;
use any_base::stream_flow::StreamReadWriteTokio;
use anyhow::anyhow;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type AcceptSenderType = async_channel::Sender<(Stream, SocketAddr, SocketAddr)>;
pub type AcceptReceiverType = async_channel::Receiver<(Stream, SocketAddr, SocketAddr)>;

pub struct Listener {
    accept_rx: AcceptReceiverType,
}

impl Listener {
    pub fn new(accept_rx: AcceptReceiverType) -> Listener {
        Listener { accept_rx }
    }

    pub async fn accept(&mut self) -> Result<(Stream, SocketAddr, SocketAddr)> {
        let accept = self.accept_rx.recv().await?;
        log::debug!("accept_rx recv");
        Ok(accept)
    }
}

#[derive(Clone)]
pub struct Publish {
    tunnel_key: String,
    server: Server,
}

impl Publish {
    pub fn new(tunnel_key: String, server: Server) -> Publish {
        Publish { tunnel_key, server }
    }
    pub async fn push_peer_stream_tokio<RW: StreamReadWriteTokio + 'static>(
        &self,
        rw: RW,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<()> {
        let rw = any_base::stream::Stream::new(rw);
        let buf_stream = any_base::io::buf_stream::BufStream::new(rw);
        self.push_peer_stream(buf_stream, local_addr, remote_addr)
            .await
    }

    pub async fn push_peer_stream<RW: StreamReadWriteFlow + 'static>(
        &self,
        rw: RW,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<()> {
        self.server
            .insert_peer_stream(
                self.tunnel_key.clone(),
                StreamFlow::new(rw, None),
                local_addr,
                remote_addr,
            )
            .await
    }
}

pub struct Accept {
    pub accept_tx: AcceptSenderType,
    pub accept_rx: AcceptReceiverType,
}

pub struct ServerContext {
    tunnel: Tunnel,
    accept: hashbrown::HashMap<String, Arc<Accept>>,
}

#[derive(Clone)]
pub struct Server {
    context: Arc<Mutex<ServerContext>>,
}

impl Server {
    pub fn new(tunnel: Tunnel) -> Server {
        Server {
            context: Arc::new(Mutex::new(ServerContext {
                tunnel,
                accept: hashbrown::HashMap::new(),
            })),
        }
    }

    pub fn tunnel_key(proto: &Protocol4, sock_addr: &SocketAddr) -> String {
        proto.to_string() + &sock_addr.to_string()
    }

    pub async fn get_accept(&self, tunnel_key: String) -> Arc<Accept> {
        let mut context = self.context.lock().await;
        let accept = context.accept.get(&tunnel_key).cloned();
        if accept.is_some() {
            accept.unwrap()
        } else {
            let (accept_tx, accept_rx) =
                async_channel::unbounded::<(Stream, SocketAddr, SocketAddr)>();
            let accept = Arc::new(Accept {
                accept_tx,
                accept_rx,
            });
            context.accept.insert(tunnel_key, accept.clone());
            accept
        }
    }

    pub async fn insert_peer_client(
        &self,
        tunnel_key: String,
        register_id: String,
        session_id: String,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<PeerClientSender> {
        let accept = self.get_accept(tunnel_key.clone()).await;
        let context = self.context.lock().await;
        let peer_client_sender = context
            .tunnel
            .insert_peer_client(
                tunnel_key,
                register_id,
                session_id,
                local_addr,
                remote_addr,
                Arc::new(AtomicUsize::new(0)),
                Some(accept.accept_tx.clone()),
                None,
            )
            .await
            .map_err(|e| anyhow!("err:.tunnel.insert_peer_client => e:{}", e))?;
        Ok(peer_client_sender)
    }

    pub async fn insert_peer_stream(
        &self,
        tunnel_key: String,
        mut stream: StreamFlow,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<()> {
        let tunnel_hello = protopack::read_tunnel_hello(&mut stream)
            .await
            .map_err(|e| anyhow!("err:read_tunnel_hello => e:{}", e))?;
        log::debug!("server tunnel_hello:{:?}", tunnel_hello);

        let peer_client_sender = self
            .insert_peer_client(
                tunnel_key,
                tunnel_hello.session_id.clone(),
                tunnel_hello.session_id.clone(),
                local_addr,
                remote_addr,
            )
            .await
            .map_err(|e| anyhow!("err:insert_peer_client => e:{}", e))?;

        PeerClient::insert_peer_stream(false, &peer_client_sender, stream)
            .await
            .map_err(|e| anyhow!("err:insert_peer_stream => e:{}", e))?;

        Ok(())
    }

    pub async fn listen(&self, proto: Protocol4, listen_addr: &SocketAddr) -> (Listener, Publish) {
        let tunnel_key = Server::tunnel_key(&proto, &listen_addr);
        let accept = self.get_accept(tunnel_key.clone()).await;
        (
            Listener::new(accept.accept_rx.clone()),
            Publish::new(tunnel_key, self.clone()),
        )
    }
}
