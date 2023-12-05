use any_tunnel::server;
use anyhow::anyhow;
use anyhow::Result;
use std::net::ToSocketAddrs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use any_base::executor_local_spawn::{ThreadRuntime};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = log4rs::init_file("examples/log4rs.yaml", Default::default()) {
        eprintln!("err:log4rs::init_file => e:{}", e);
        return Err(anyhow!("err:log4rs::init_fil"))?;
    }

    let listen_addr = "127.0.0.1:28080".to_socket_addrs()?.next().unwrap();
    log::info!("listen_addr:{}", listen_addr);
    let listener = TcpListener::bind(listen_addr.clone()).await?;
    let server = server::Server::new();

    let (mut listen, publish) = server.listen().await;
    tokio::spawn(async move {
        loop {
            let (mut stream, _, _) = listen.accept().await.unwrap();
            log::info!("tunnel2 listen.accept");
            tokio::spawn(async move {
                let ret: Result<()> = async {
                    stream.read_i32().await?;
                    let mut num = 0;
                    let mut w_n = 0;
                    let slice = [0u8; 8192];
                    loop {
                        num += 1;
                        let n = stream.write(&slice).await?;
                        if n <= 0 {
                            log::info!("close");
                            break;
                        }
                        w_n += n;
                        log::trace!("write w_n:{}", w_n);
                        if num > 10000000 {
                            break;
                        }
                    }
                    Ok(())
                }
                .await;
                ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
                stream.close();
                log::info!("stream.close()");
            });
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;

        let local_addr = stream.local_addr()?;
        let remote_addr = stream.peer_addr()?;
        log::info!("listener.accept()");

        let publish = publish.clone();
        tokio::spawn(async move {
            if let Err(e) = publish
                .push_peer_stream(stream, local_addr, remote_addr,
                                  Arc::new(Box::new(ThreadRuntime))
                .await
            {
                log::error!("err: server stream => e:{}", e);
            }
            log::info!("server stream close");
        });
    }
}
