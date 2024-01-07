use any_base::executor_local_spawn::ThreadRuntime;
use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use any_base::util::StreamMsg;
use any_tunnel::client;
use any_tunnel::peer_stream_connect::PeerStreamConnect;
use any_tunnel::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Stream {
    s: TcpStream,
}

impl Stream {
    pub fn new(s: TcpStream) -> Stream {
        Stream { s }
    }
}

impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&self.s).poll_write_ready(cx)
    }
    fn poll_try_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        use tokio::io::AsyncRead;
        let ret = Pin::new(&mut self.s).poll_read(cx, buf);
        match ret {
            Poll::Pending => {
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(ret) => {
                if ret.is_err() {
                    ret?;
                }
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamMsg>> {
        return Poll::Ready(Ok(StreamMsg::new()));
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamMsg>> {
        return Poll::Ready(Ok(StreamMsg::new()));
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl any_base::io::async_write_msg::AsyncWriteMsg for Stream {
    fn poll_write_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        return Poll::Ready(Ok(0));
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.s).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.s).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.s.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.s).poll_shutdown(cx)
    }
}

pub struct PeerStreamConnectTcp {
    addr: ArcString,
    max_stream_size: usize,
    min_stream_cache_size: usize,
    channel_size: usize,
}

impl PeerStreamConnectTcp {
    pub fn new(
        addr: String,
        max_stream_size: usize,
        min_stream_cache_size: usize,
        channel_size: usize,
    ) -> PeerStreamConnectTcp {
        PeerStreamConnectTcp {
            addr: ArcString::new(addr),
            max_stream_size,
            min_stream_cache_size,
            channel_size,
        }
    }
}

#[async_trait]
impl PeerStreamConnect for PeerStreamConnectTcp {
    async fn connect(&self) -> Result<(StreamFlow, SocketAddr, SocketAddr)> {
        let stream = TcpStream::connect(self.addr().await?)
            .await
            .map_err(|e| anyhow!("err:TcpStream::connect => e:{}", e))?;
        let local_addr = stream.local_addr()?;
        let remote_addr = stream.peer_addr()?;
        let stream = Stream::new(stream);
        Ok((StreamFlow::new(0, stream), local_addr, remote_addr))
    }
    async fn addr(&self) -> Result<SocketAddr> {
        let connect_addr = self
            .addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "err:empty address"))
            .map_err(|e| anyhow!("err:to_socket_addrs => e:{}", e))?;
        Ok(connect_addr)
    }
    async fn host(&self) -> ArcString {
        self.addr.clone()
    }
    async fn protocol4(&self) -> Protocol4 {
        Protocol4::TCP
    }
    async fn protocol7(&self) -> String {
        "tcp".to_string()
    }
    async fn max_stream_size(&self) -> usize {
        self.max_stream_size
    }
    async fn min_stream_cache_size(&self) -> usize {
        self.min_stream_cache_size
    }
    async fn channel_size(&self) -> usize {
        self.channel_size
    }
    async fn stream_send_timeout(&self) -> usize {
        10
    }
    async fn stream_recv_timeout(&self) -> usize {
        10
    }
    async fn key(&self) -> Result<String> {
        Ok(format!("{}{}", self.protocol7().await, self.addr().await?))
    }
    async fn is_tls(&self) -> bool {
        false
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    if let Err(_) = log4rs::init_file("conf/log4rs.yaml", Default::default()) {
        if let Err(e) = log4rs::init_file("log4rs.yaml", Default::default()) {
            eprintln!("err:log4rs::init_file => e:{}", e);
            return Err(anyhow!("err:log4rs::init_file"))?;
        }
    }
    let mut number = 1;
    loop {
        let ret: Result<()> = async {
            let connect_addr = "127.0.0.1:28080".to_string();
            log::info!("connect_addr:{}", connect_addr);
            let client = client::Client::new();
            let (mut stream, _, _) = client
                .connect(
                    None,
                    Arc::new(Box::new(PeerStreamConnectTcp::new(connect_addr, 10, 0, 64))),
                    None,
                    Arc::new(Box::new(ThreadRuntime)),
                )
                .await?;

            let mut w_n = 0;
            let mut r_n = 0;
            let ret: Result<()> = async {
                let mut slice = [0u8; 8192];
                loop {
                    let n = stream.write(&slice).await?;
                    if n <= 0 {
                        log::info!("write close");
                        break;
                    }
                    w_n += n;
                    //
                    // let n = stream.read_exact(&mut slice[0..n]).await?;
                    // if n <= 0 {
                    //     log::info!("read close");
                    //     break;
                    // }
                    // r_n += n;
                    // //log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
                    if w_n > 8192 * 1000 {
                        log::info!("w_n end");
                        break;
                    }
                }
                let n = stream.write(&slice).await?;
                if n <= 0 {
                    log::info!("write close");
                }
                w_n += n;
                //log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
                stream.flush().await?;
                stream.shutdown().await?;
                let n = stream.read_exact(&mut slice).await?;
                if n <= 0 {
                    log::info!("read_exact close");
                }
                r_n += n;
                //log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
                Ok(())
            }
            .await;
            ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
            //log::info!("stream r_n:{}, w_n:{}", r_n, w_n);
            stream.close();
            //log::info!("stream.close()");
            Ok(())
        }
        .await;
        ret.unwrap_or_else(|e| log::error!("err:stream => e:{}", e));
        number += 1;
        log::info!("number:{}", number);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
