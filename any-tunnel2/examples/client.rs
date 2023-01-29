use any_tunnel2::client;
use any_tunnel2::peer_stream_connect::PeerStreamConnectTcp;
use any_tunnel2::tunnel;
use anyhow::anyhow;
use anyhow::Result;
use std::error::Error;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
//curl http://www.upstream.cn:18080/5g.b --output a --limit-rate 100k
#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    if let Err(e) = log4rs::init_file("examples/log4rs.yaml", Default::default()) {
        eprintln!("err:log4rs::init_file => e:{}", e);
        return Err(anyhow!("err:log4rs::init_fil"))?;
    }
    let ret: Result<()> = async {
        let connect_addr = "127.0.0.1:28081".to_string();
        log::info!("connect_addr:{}", connect_addr);

        let tunnel = tunnel::Tunnel::start().await;
        let client = client::Client::new(tunnel, Arc::new(AtomicUsize::new(10)));
        let (mut stream, _, _) = client
            .connect(Arc::new(Box::new(PeerStreamConnectTcp::new(connect_addr))))
            .await?;

        let ret: Result<()> = async {
            stream.write_i32(1).await?;
            loop {
                let n = stream.read_i32().await?;
                log::info!("read n:{}", n);
                if n > 100000000 {
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
