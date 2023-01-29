use any_tunnel::client;
use any_tunnel::peer_stream_connect::PeerStreamConnectTcp;
use anyhow::anyhow;
use anyhow::Result;
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    if let Err(e) = log4rs::init_file("examples/log4rs.yaml", Default::default()) {
        eprintln!("err:log4rs::init_file => e:{}", e);
        return Err(anyhow!("err:log4rs::init_fil"))?;
    }
    let ret: Result<()> = async {
        let connect_addr = "127.0.0.1:28080".to_string();
        log::info!("connect_addr:{}", connect_addr);
        let client = client::Client::new();
        let (mut stream, _, _) = client
            .connect(
                Arc::new(Box::new(PeerStreamConnectTcp::new(connect_addr, 5, 50, 64))),
                None,
            )
            .await?;

        let ret: Result<()> = async {
            let mut num = 0;
            let mut r_n = 0;
            stream.write_i32(1).await?;
            let mut slice = [0u8; 8192];
            loop {
                num += 1;
                let n = stream.read(&mut slice).await?;
                if n <= 0 {
                    log::info!("close");
                    break;
                }
                r_n += n;
                log::trace!("read r_n:{}", r_n);
                if num > 100000000 {
                    break;
                }
            }
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
        stream.close();
        log::info!("stream.close()");
        Ok(())
    }
    .await;
    ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    Ok(())
}
