use any_base::macros::Runtime::ThreadRuntime;
use any_base::stream_flow::StreamFlow;
use any_tunnel::server;
use anyhow::anyhow;
use anyhow::Result;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicU64, Ordering, AtomicI64};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PACK_MAX_NUM: i64 = 10000000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("pwd:{:?}", std::env::current_dir()?);
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

    let (mut listen, publish) = server.listen(ThreadRuntime).await;

    let accept_num = Arc::new(AtomicU64::new(0));
    let stream_close_num = Arc::new(AtomicU64::new(0));
    let err_num = Arc::new(AtomicU64::new(0));
    let accept_num2 = Arc::new(AtomicU64::new(0));
    let stream_close_num2 = Arc::new(AtomicU64::new(0));
    {

        let accept_num = accept_num.clone();
        let stream_close_num = stream_close_num.clone();
        let err_num = err_num.clone();
        let accept_num2 = accept_num2.clone();
        let stream_close_num2 = stream_close_num2.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                log::info!(
                    "tunnel server accept_num:{}, stream_close_num:{}, err_num:{}, accept_num2:{}, stream_close_num2:{}",
                    accept_num.load(Ordering::Relaxed),
                    stream_close_num.load(Ordering::Relaxed),
                    err_num.load(Ordering::Relaxed), accept_num2.load(Ordering::Relaxed), stream_close_num2.load(Ordering::Relaxed));
            }
        });
    }
    {
        let accept_num = accept_num.clone();
        let stream_close_num = stream_close_num.clone();
        let err_num = err_num.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _, _, _) = listen.accept().await.unwrap();
                {
                    let accept_num = accept_num.fetch_add(1, Ordering::Relaxed) + 1;
                }
                let accept_num = accept_num.clone();
                let stream_close_num = stream_close_num.clone();
                {
                    let err_num = err_num.clone();
                    tokio::spawn(async move {
                        let stream = StreamFlow::new(stream, None);
                        let (mut r, mut w) = any_base::io::split::split(stream);

                        let wg = awaitgroup::WaitGroup::new();
                        let read_num = Arc::new(AtomicI64::new(0));
                        {
                            let read_num = read_num.clone();
                            let err_num = err_num.clone();
                            let rwi = wg.worker().add();
                            tokio::spawn(async move {
                                let ret: Result<()> = async {
                                    let mut r = any_base::io_rb::buf_reader::BufReader::new(r);
                                    let mut read_data = [0u8; 8];
                                    let mut last_n = -1 as i64;
                                    loop {
                                        let size = r.read_exact(&mut read_data).await?;
                                        if size != read_data.len() {
                                            return Err(anyhow::anyhow!("read_exact"));
                                        }
                                        let n = i64::from_le_bytes(read_data);
                                        if i64::max_value() == n {
                                            if last_n != PACK_MAX_NUM {
                                                return Err(anyhow::anyhow!(
                                                    "last_n:{} != PACK_MAX_NUM:{}",
                                                    last_n, PACK_MAX_NUM
                                                ));
                                            }
                                            return Ok(());
                                        }

                                        if n % 1000 == 0 {
                                            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                        }

                                        if last_n < 0 {
                                            last_n = n;
                                            read_num.store(last_n, Ordering::Relaxed);
                                        } else {
                                            if n != last_n + 1 {
                                                return Err(anyhow::anyhow!(
                                                    "n:{} != last_n + 1:{}",
                                                    n,
                                                    last_n + 1
                                                ));
                                            }
                                            last_n = n;
                                            read_num.store(last_n, Ordering::Relaxed);
                                        }
                                    }
                                }
                                .await;
                                if let Err(e) = ret {
                                    if !e.to_string().contains("close") {
                                        log::error!("err:stream => e:{}", e);
                                        err_num.fetch_add(1, Ordering::Relaxed);
                                    }
                                }

                                rwi.done()
                            });
                        }
                        let write_num = Arc::new(AtomicI64::new(0));
                        {
                            let write_num = write_num.clone();
                            let err_num = err_num.clone();
                            let wwi = wg.worker().add();
                            tokio::spawn(async move {
                                let mut w = any_base::io_rb::buf_writer::BufWriter::new(w);
                                let ret: Result<()> = async {
                                    let mut n = -1 as i64;
                                    loop {
                                        n = n + 1;
                                        if n > PACK_MAX_NUM {
                                            w.write_all(i64::max_value().to_le_bytes().as_slice())
                                                .await?;
                                            w.flush().await?;
                                            return Ok(());
                                        }
                                        write_num.store(n, Ordering::Relaxed);
                                        w.write_all(n.to_le_bytes().as_slice()).await?;
                                    }
                                }
                                .await;
                                if let Err(e) = ret {
                                    if !e.to_string().contains("close") {
                                        log::error!("err:stream => e:{}", e);
                                        err_num.fetch_add(1, Ordering::Relaxed);
                                    }
                                }

                                wwi.done()
                            });
                        }

                        'doop: loop {
                            tokio::select! {
                                _ = wg.wait() => {
                                    break 'doop;
                                },
                                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                                    log::info!(
                                    "tunnel server accept_num:{}, stream_close_num:{}, err_num:{}, read_num:{}, write_num:{}",
                                    accept_num.load(Ordering::Relaxed),
                                    stream_close_num.load(Ordering::Relaxed),
                                    err_num.load(Ordering::Relaxed), read_num.load(Ordering::Relaxed), write_num.load(Ordering::Relaxed));
                                }
                            }
                        }

                        let stream_close_num = stream_close_num.fetch_add(1, Ordering::Relaxed) + 1;
                    });
                }
            }
        });
    }

    let accept_num = accept_num2.clone();
    let stream_close_num = stream_close_num2.clone();
    loop {
        let (stream, _) = listener.accept().await?;
        {
            let accept_num = accept_num.fetch_add(1, Ordering::Relaxed) + 1;
        }

        let local_addr = stream.local_addr()?;
        let remote_addr = stream.peer_addr()?;

        let publish = publish.clone();
        let stream_close_num = stream_close_num.clone();
        let accept_num = accept_num.clone();
        let err_num = err_num.clone();
        tokio::spawn(async move {
            if let Err(e) = publish
                .push_peer_stream_tokio(stream, local_addr, remote_addr, None)
                .await
            {
                log::error!("tunnel err: server publish stream => e:{}", e);
                err_num.fetch_add(1, Ordering::Relaxed);
            }
            let stream_close_num = stream_close_num.fetch_add(1, Ordering::Relaxed) + 1;
        });
    }
}
