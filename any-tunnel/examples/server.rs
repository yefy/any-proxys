use any_base::executor_local_spawn::ThreadRuntime;
use any_tunnel::server;
use anyhow::anyhow;
use anyhow::Result;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(_) = log4rs::init_file("conf/log4rs.yaml", Default::default()) {
        if let Err(e) = log4rs::init_file("log4rs.yaml", Default::default()) {
            eprintln!("err:log4rs::init_file => e:{}", e);
            return Err(anyhow!("err:log4rs::init_file"))?;
        }
    }

    let listen_addr = "127.0.0.1:28080".to_socket_addrs()?.next().unwrap();
    log::info!("listen_addr:{}", listen_addr);
    let listener = TcpListener::bind(listen_addr.clone()).await?;
    let server = server::Server::new();

    let (mut listen, publish) = server.listen(Arc::new(Box::new(ThreadRuntime))).await;
    tokio::spawn(async move {
        loop {
            let (mut stream, _, _, _) = listen.accept().await.unwrap();
            log::info!("tunnel2 listen.accept");
            tokio::spawn(async move {
                let mut r_n = 0;
                let mut w_n = 0;
                let ret: Result<()> = async {
                    let mut slice = [0u8; 8192];
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        let n = stream.read(&mut slice).await?;
                        if n <= 0 {
                            log::info!("read close");
                            break;
                        }
                        // r_n += n;
                        // stream.write_all(&slice[0..n]).await?;
                        // w_n += n;
                        // log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
                    }
                    log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
                    Ok(())
                }
                .await;
                ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
                log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
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
                .push_peer_stream_tokio(stream, local_addr, remote_addr, None)
                .await
            {
                log::error!("err: server stream => e:{}", e);
            }
            log::info!("server stream close");
        });
    }
}
