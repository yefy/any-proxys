use crate::tunnel::server as tunnel_server;
use crate::tunnel2::server as tunnel2_server;
use crate::util;
use crate::Protocol7;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use any_tunnel::server as any_tunnel_server;
use any_tunnel2::server as any_tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug)]
pub struct ServerStreamInfo {
    pub protocol7: Protocol7,
    pub remote_addr: SocketAddr,
    pub local_addr: Option<SocketAddr>,
    pub domain: Option<ArcString>,
    pub is_tls: bool,
    pub raw_fd: i32,
}

#[async_trait]
pub trait Server: Send + Sync {
    async fn listen(&self) -> Result<Box<dyn Listener>>;
    fn listen_addr(&self) -> Result<SocketAddr>;
    fn sni(&self) -> Option<util::Sni>;
    fn set_sni(&self, _: util::Sni);
    fn protocol7(&self) -> Protocol7;
    fn stream_send_timeout(&self) -> usize;
    fn stream_recv_timeout(&self) -> usize;
    fn is_tls(&self) -> bool;
}

#[async_trait]
pub trait Listener: Send + Sync {
    async fn accept(&mut self) -> Result<(Box<dyn Connection>, bool)>;
}

#[async_trait]
pub trait Connection: Send {
    async fn stream(&mut self) -> Result<Option<(StreamFlow, ServerStreamInfo)>>;
}

pub async fn accept<A: Listener + ?Sized>(
    shutdown_rx: &mut broadcast::Receiver<bool>,
    shutdown_rx2: &mut broadcast::Receiver<()>,
    listen: &mut Box<A>,
) -> Result<(Option<(Box<dyn Connection>, bool)>, bool)> {
    tokio::select! {
        biased;
        connection = listen.accept() => {
            let connection = connection?;
            return Ok((Some(connection), false));
        }
        is_fast_shutdown = shutdown_rx.recv() => {
            if is_fast_shutdown.is_err() {
                return Ok((None, true))
            } else {
                return Ok((None, is_fast_shutdown.unwrap()))
            }
        }
        _ = shutdown_rx2.recv() => {
            return Ok((None, false));
        }
        else => {
            return Err(anyhow!("err:accept"));
        }
    }
}

pub async fn stream<C: Connection + ?Sized>(
    shutdown_rx: &mut broadcast::Receiver<bool>,
    shutdown_rx2: &mut broadcast::Receiver<()>,
    connection: &mut Box<C>,
) -> Result<Option<(StreamFlow, ServerStreamInfo)>> {
    tokio::select! {
        biased;
        stream = connection.stream() => {
            let stream = stream?;
            return Ok(stream);
        }
        _ = shutdown_rx.recv() => {
            return Ok(None);
        }
        _ = shutdown_rx2.recv() => {
            return Ok(None);
        }
        else => {
            return Err(anyhow!("err:stream"));
        }
    }
}

pub async fn get_listens(
    tunnel_listen: Option<any_tunnel_server::Listener>,
    tunnel2_listen: Option<any_tunnel2_server::Listener>,
    listen_server: Arc<Box<dyn Server>>,
) -> Result<Vec<Box<dyn Listener>>> {
    let mut listens = Vec::with_capacity(5);
    if tunnel_listen.is_some() {
        let tunnel_listen: Box<dyn Listener> = Box::new(tunnel_server::Listener::new(
            listen_server.protocol7().to_tunnel_protocol7()?,
            tunnel_listen.unwrap(),
            listen_server.stream_send_timeout(),
            listen_server.stream_recv_timeout(),
            listen_server.is_tls(),
        )?);
        listens.push(tunnel_listen);
    }

    if tunnel2_listen.is_some() {
        let tunnel2_listen: Box<dyn Listener> = Box::new(tunnel2_server::Listener::new(
            listen_server.protocol7().to_tunnel2_protocol7()?,
            tunnel2_listen.unwrap(),
            listen_server.stream_send_timeout(),
            listen_server.stream_recv_timeout(),
            listen_server.is_tls(),
        )?);
        listens.push(tunnel2_listen);
    }

    let listen = listen_server
        .listen()
        .await
        .map_err(|e| anyhow!("err:listen_server.listen => e:{}", e))?;

    listens.push(listen);
    return Ok(listens);
}

pub async fn listen<S, F>(
    #[cfg(feature = "anyspawn-count")] name: String,
    executors: ExecutorsLocal,
    shutdown_timeout: u64,
    listen_shutdown_tx: broadcast::Sender<()>,
    listen_server: Arc<Box<dyn Server>>,
    mut listen: Box<dyn Listener>,
    service: S,
) -> Result<()>
where
    S: FnOnce(StreamFlow, ServerStreamInfo, ExecutorsLocal) -> F + 'static + Clone + Send,
    F: Future<Output = Result<()>> + 'static + Send,
{
    let mut executor = ExecutorLocalSpawn::new(executors.clone());

    let local_addr = listen_server
        .listen_addr()
        .map_err(|e| anyhow!("err:listen_server.listen_addr => e:{}", e))?;
    if log::log_enabled!(target: "main", log::Level::Debug) {
        log::debug!(target: "main",
            "start listen thread_id:{:?}, Protocol7:{}, listen_addr:{}",
            std::thread::current().id(),
            listen_server.protocol7().to_string(),
            local_addr
        );
    }
    let mut accept_shutdown_thread_tx = executors.context.shutdown_thread_tx.subscribe();
    let mut accept_listen_shutdown_tx = listen_shutdown_tx.subscribe();
    let is_fast_shutdown = loop {
        let (accept, is_fast_shutdown) = accept(
            &mut accept_shutdown_thread_tx,
            &mut accept_listen_shutdown_tx,
            &mut listen,
        )
        .await
        .map_err(|e| anyhow!("err:accept => e:{}", e))?;
        if accept.is_none() {
            if is_fast_shutdown {
                executor.send("listen send", is_fast_shutdown)
            }
            break is_fast_shutdown;
        }
        let (mut connection, is_async) = accept.unwrap();
        if !is_async {
            let local_addr = local_addr.clone();
            #[cfg(feature = "anyspawn-count")]
            let name = name.clone();
            let service = service.clone();
            executor._start(
                #[cfg(feature = "anyspawn-count")]
                Some(format!(
                    "{}:{} => {}:{}",
                    file!(),
                    line!(),
                    name,
                    local_addr
                )),
                move |executors| async move {
                    let stream = connection
                        .stream()
                        .await
                        .map_err(|e| anyhow!("err:connection.stream => e:{}", e))?;
                    if stream.is_none() {
                        return Ok(());
                    }
                    let (stream, mut server_stream_info) = stream.unwrap();
                    if server_stream_info.local_addr.is_none() {
                        server_stream_info.local_addr = Some(local_addr);
                    }
                    if let Err(e) = service(stream, server_stream_info, executors).await {
                        log::error!("err:stream => e:{}", e);
                    }
                    Ok(())
                },
            );
        } else {
            let local_addr = local_addr.clone();
            #[cfg(feature = "anyspawn-count")]
            let name = name.clone();
            let service = service.clone();
            let connection_shutdown_thread_tx = executors.context.shutdown_thread_tx.clone();
            let connection_listen_shutdown_tx = listen_shutdown_tx.clone();
            executor._start(
                #[cfg(feature = "anyspawn-count")]
                None,
                move |executors| async move {
                    let mut stream_shutdown_thread_tx = connection_shutdown_thread_tx.subscribe();
                    let mut stream_listen_shutdown_tx = connection_listen_shutdown_tx.subscribe();
                    loop {
                        let stream = stream(
                            &mut stream_shutdown_thread_tx,
                            &mut stream_listen_shutdown_tx,
                            &mut connection,
                        )
                        .await
                        .map_err(|e| anyhow!("err:stream => e:{}", e))?;
                        if stream.is_none() {
                            return Ok(());
                        }
                        let local_addr = local_addr.clone();
                        #[cfg(feature = "anyspawn-count")]
                        let name = name.clone();
                        let service = service.clone();
                        executors._start(
                            #[cfg(feature = "anyspawn-count")]
                            Some(format!(
                                "{}:{} => {}:{}",
                                file!(),
                                line!(),
                                name,
                                local_addr
                            )),
                            move |executor| async move {
                                let (stream, mut server_stream_info) = stream.unwrap();
                                if server_stream_info.local_addr.is_none() {
                                    server_stream_info.local_addr = Some(local_addr);
                                }
                                if let Err(e) = service(stream, server_stream_info, executor).await
                                {
                                    log::error!("err:stream => e:{}", e);
                                }
                                Ok(())
                            },
                        );
                    }
                },
            );
        }
    };

    executor
        .stop("listen stop", is_fast_shutdown, shutdown_timeout)
        .await;
    if log::log_enabled!(target: "main", log::Level::Debug) {
        log::debug!(target: "main",
            "close listen thread_id:{:?}, Protocol7{}, listen_addr:{}",
            std::thread::current().id(),
            listen_server.protocol7().to_string(),
            local_addr
        );
    }

    Ok(())
}
