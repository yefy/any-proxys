use crate::stream::stream_flow;
use crate::tunnel::server as tunnel_server;
use crate::tunnel2::server as tunnel2_server;
use crate::util;
use crate::Protocol7;
use any_tunnel::server as any_tunnel_server;
use any_tunnel2::server as any_tunnel2_server;
use async_executors::Timer;
use async_trait::async_trait;
use awaitgroup::WaitGroup;
use futures_util::task::LocalSpawnExt;
use std::future::Future;
use std::net::SocketAddr;
use std::rc::Rc;
use tokio::sync::broadcast;

#[async_trait(?Send)]
pub trait Server {
    async fn listen(&self) -> anyhow::Result<Box<dyn Listener>>;
    fn listen_addr(&self) -> anyhow::Result<SocketAddr>;
    fn sni(&self) -> Option<std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>>;
    fn set_sni(&self, _: std::sync::Arc<util::rustls::ResolvesServerCertUsingSNI>);
    fn protocol7(&self) -> Protocol7;
}

#[async_trait(?Send)]
pub trait Listener {
    async fn accept(&mut self) -> anyhow::Result<(Box<dyn Connection>, bool)>;
}

#[async_trait(?Send)]
pub trait Connection {
    async fn stream(
        &mut self,
    ) -> anyhow::Result<
        Option<(
            Protocol7,
            stream_flow::StreamFlow,
            SocketAddr,
            Option<String>,
        )>,
    >;
}

pub async fn accept<A: Listener + ?Sized>(
    shutdown_rx: &mut broadcast::Receiver<bool>,
    shutdown_rx2: &mut broadcast::Receiver<()>,
    listenner: &mut Box<A>,
    listenner2: &mut Box<A>,
    listenner3: &mut Box<A>,
) -> anyhow::Result<(Option<(Box<dyn Connection>, bool)>, bool)> {
    loop {
        tokio::select! {
            biased;
            connection = listenner.accept() => {
                let connection = connection?;
                return Ok((Some(connection), false));
            }
            connection = listenner2.accept() => {
                let connection = connection?;
                return Ok((Some(connection), false));
            }
            connection = listenner3.accept() => {
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
                return Err(anyhow::anyhow!("err:accept"));
            }
        }
    }
}

pub async fn stream<C: Connection + ?Sized>(
    shutdown_rx: &mut broadcast::Receiver<bool>,
    shutdown_rx2: &mut broadcast::Receiver<()>,
    connection: &mut Box<C>,
) -> anyhow::Result<
    Option<(
        Protocol7,
        stream_flow::StreamFlow,
        SocketAddr,
        Option<String>,
    )>,
> {
    loop {
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
                return Err(anyhow::anyhow!("err:stream"));
            }
        }
    }
}

pub async fn listen<S, F>(
    tunnel_listen: any_tunnel_server::Listener,
    tunnel2_listen: any_tunnel2_server::Listener,
    shutdown_timeout: u64,
    listen_server: Rc<Box<dyn Server>>,
    executor: async_executors::TokioCt,
    listen_shutdown_tx: broadcast::Sender<()>,
    shutdown_thread_tx: broadcast::Sender<bool>,
    service: S,
) -> anyhow::Result<()>
where
    S: FnMut(
            Protocol7,
            stream_flow::StreamFlow,
            SocketAddr,
            SocketAddr,
            Option<String>,
            broadcast::Sender<()>,
        ) -> F
        + 'static
        + Clone,
    F: Future<Output = anyhow::Result<()>> + 'static,
{
    let (shutdown_tx, _) = broadcast::channel::<()>(100);
    let wait_conn_groups = WaitGroup::new();

    let local_addr = listen_server.listen_addr()?;

    let mut tunnel_listen: Box<dyn Listener> = Box::new(tunnel_server::Listener::new(
        listen_server.protocol7().to_tunnel_protocol7()?,
        tunnel_listen,
    )?);

    let mut tunnel2_listen: Box<dyn Listener> = Box::new(tunnel2_server::Listener::new(
        listen_server.protocol7().to_tunnel2_protocol7()?,
        tunnel2_listen,
    )?);

    let mut accept_shutdown_thread_tx = shutdown_thread_tx.subscribe();
    let mut accept_listen_shutdown_tx = listen_shutdown_tx.subscribe();
    let mut listen = listen_server.listen().await?;
    loop {
        let (accept, is_fast_shutdown) = accept(
            &mut accept_shutdown_thread_tx,
            &mut accept_listen_shutdown_tx,
            &mut listen,
            &mut tunnel_listen,
            &mut tunnel2_listen,
        )
        .await?;
        if accept.is_none() {
            if is_fast_shutdown {
                let _ = shutdown_tx.send(());
            }
            break;
        }
        let (mut connection, is_async) = accept.unwrap();
        if !is_async {
            let local_addr = local_addr.clone();
            let mut service = service.clone();
            let shutdown_tx = shutdown_tx.clone();
            let worker = wait_conn_groups.worker().add();
            executor
                .spawn_local(async move {
                    scopeguard::defer! {
                        worker.done();
                    }

                    let _: anyhow::Result<()> = async {
                        let stream = connection.stream().await?;
                        if stream.is_none() {
                            return Ok(());
                        }
                        let (protocol_name, stream, remote_addr, domain) = stream.unwrap();
                        if let Err(e) = service(
                            protocol_name,
                            stream,
                            local_addr,
                            remote_addr,
                            domain,
                            shutdown_tx,
                        )
                        .await
                        {
                            log::error!("err:stream => e:{}", e);
                        }
                        Ok(())
                    }
                    .await;
                })
                .unwrap_or_else(|e| log::error!("{}", e));
        } else {
            let local_addr = local_addr.clone();
            let service = service.clone();
            let shutdown_tx = shutdown_tx.clone();
            let connection_shutdown_thread_tx = shutdown_thread_tx.clone();
            let connection_listen_shutdown_tx = listen_shutdown_tx.clone();
            let sub_executor = executor.clone();
            let worker = wait_conn_groups.worker().add();
            let sub_worker = wait_conn_groups.worker();
            executor
                .spawn_local(async move {
                    scopeguard::defer! {
                        worker.done();
                    }
                    let _: anyhow::Result<()> = async {
                        let mut stream_shutdown_thread_tx =
                            connection_shutdown_thread_tx.subscribe();
                        let mut stream_listen_shutdown_tx =
                            connection_listen_shutdown_tx.subscribe();
                        loop {
                            let stream = stream(
                                &mut stream_shutdown_thread_tx,
                                &mut stream_listen_shutdown_tx,
                                &mut connection,
                            )
                            .await?;
                            if stream.is_none() {
                                return Ok(());
                            }

                            let local_addr = local_addr.clone();
                            let mut service = service.clone();
                            let shutdown_tx = shutdown_tx.clone();
                            let worker = sub_worker.worker().add();
                            sub_executor
                                .spawn_local(async move {
                                    scopeguard::defer! {
                                        worker.done();
                                    }
                                    let _: anyhow::Result<()> = async {
                                        let (protocol_name, stream, remote_addr, domain) =
                                            stream.unwrap();
                                        if let Err(e) = service(
                                            protocol_name,
                                            stream,
                                            local_addr,
                                            remote_addr,
                                            domain,
                                            shutdown_tx,
                                        )
                                        .await
                                        {
                                            log::error!("err:stream => e:{}", e);
                                        }
                                        Ok(())
                                    }
                                    .await;
                                })
                                .unwrap_or_else(|e| log::error!("{}", e));
                        }
                    }
                    .await;
                })
                .unwrap_or_else(|e| log::error!("{}", e));
        }
    }

    loop {
        tokio::select! {
            biased;
            _ = executor.sleep(std::time::Duration::from_secs(shutdown_timeout)) => {
                let _ = shutdown_tx.send(());
                log::info!("shutdown_timeout:{}s", shutdown_timeout);
            }
            _ = wait_conn_groups.wait() => {
                log::info!("wait_conn_groups.wait thread_id:{:?}, listen_addr:{}", std::thread::current().id(), local_addr);
                break;
            }
            else => {
                break;
            }
        }
    }

    Ok(())
}
