use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::macros::Runtime::ThreadRuntime;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use any_base::util::StreamReadMsg;
use any_tunnel::client;
use any_tunnel::peer_stream_connect::PeerStreamConnect;
use any_tunnel::Protocol4;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[cfg(unix)]
use crate::proxy::sendfile;
#[cfg(unix)]
use std::future::Future;
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn default_tcp_config_name() -> String {
    "tcp_config_default".to_string()
}
fn default_tcp_send_buffer() -> usize {
    0
}
fn default_tcp_recv_buffer() -> usize {
    0
}
fn default_tcp_nodelay() -> bool {
    false
}
fn default_tcp_nopush() -> bool {
    false
}
fn default_tcp_send_timeout() -> usize {
    60
}
fn default_tcp_recv_timeout() -> usize {
    60
}
fn default_tcp_connect_timeout() -> usize {
    60
}
fn default_sendfile_max_write_size() -> usize {
    1048576
}
fn default_sendfile_eagain_sleep_mil_time() -> u64 {
    1
}
fn default_sendfile_would_block_max_sleep_count() -> usize {
    30
}
fn default_sendfile_would_block_sleep_mil() -> u64 {
    10
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    #[serde(default = "default_tcp_config_name")]
    pub tcp_config_name: String,
    #[serde(default = "default_tcp_send_buffer")]
    pub tcp_send_buffer: usize,
    #[serde(default = "default_tcp_recv_buffer")]
    pub tcp_recv_buffer: usize,
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
    #[serde(default = "default_tcp_nopush")]
    pub tcp_nopush: bool,
    #[serde(default = "default_tcp_send_timeout")]
    pub tcp_send_timeout: usize,
    #[serde(default = "default_tcp_recv_timeout")]
    pub tcp_recv_timeout: usize,
    #[serde(default = "default_tcp_connect_timeout")]
    pub tcp_connect_timeout: usize,
    #[serde(default = "default_sendfile_max_write_size")]
    pub sendfile_max_write_size: usize,
    #[serde(default = "default_sendfile_eagain_sleep_mil_time")]
    pub sendfile_eagain_sleep_mil_time: u64,
    #[serde(default = "default_sendfile_would_block_max_sleep_count")]
    pub sendfile_would_block_max_sleep_count: usize,
    #[serde(default = "default_sendfile_would_block_sleep_mil")]
    pub sendfile_would_block_sleep_mil: u64,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            tcp_config_name: default_tcp_config_name(),
            tcp_send_buffer: default_tcp_send_buffer(),
            tcp_recv_buffer: default_tcp_recv_buffer(),
            tcp_nodelay: default_tcp_nodelay(),
            tcp_nopush: default_tcp_nopush(),
            tcp_send_timeout: default_tcp_send_timeout(),
            tcp_recv_timeout: default_tcp_recv_timeout(),
            tcp_connect_timeout: default_tcp_connect_timeout(),
            sendfile_max_write_size: default_sendfile_max_write_size(),
            sendfile_eagain_sleep_mil_time: default_sendfile_eagain_sleep_mil_time(),
            sendfile_would_block_max_sleep_count: default_sendfile_would_block_max_sleep_count(),
            sendfile_would_block_sleep_mil: default_sendfile_would_block_sleep_mil(),
        }
    }
}

pub struct Stream {
    s: TcpStream,
    fd: i32,
    _config: Arc<TcpConfig>,
    #[cfg(unix)]
    sendfile_future:
        Option<Pin<Box<dyn Future<Output = io::Result<usize>> + std::marker::Send + Sync>>>,
    #[cfg(unix)]
    sendfile_zero_count: Arc<AtomicUsize>,
}

impl Stream {
    pub fn new(s: TcpStream, _config: Arc<TcpConfig>) -> Stream {
        #[cfg(unix)]
        use std::os::fd::AsRawFd;
        #[cfg(unix)]
        let fd = s.as_raw_fd();
        #[cfg(not(unix))]
        let fd = 0;
        Stream {
            s,
            fd,
            _config,
            #[cfg(unix)]
            sendfile_future: None,
            #[cfg(unix)]
            sendfile_zero_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn disable_warning(&mut self) {}

    #[cfg(unix)]
    pub async fn sendfile(
        socket_fd: i32,
        file_fd: i32,
        seek: u64,
        size: usize,
        _config: &TcpConfig,
        sendfile_zero_count: Arc<AtomicUsize>,
    ) -> io::Result<usize> {
        let ret = sendfile::sendfile(socket_fd, file_fd, seek, size, _config).await;
        if _config.sendfile_would_block_max_sleep_count > 0
            && _config.sendfile_would_block_sleep_mil > 0
        {
            match &ret {
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        let count = sendfile_zero_count.fetch_add(1, Ordering::Relaxed) + 1;
                        if count >= _config.sendfile_would_block_max_sleep_count {
                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                _config.sendfile_would_block_sleep_mil,
                            ))
                            .await;
                        }
                    }
                }
                _ => {
                    sendfile_zero_count.store(0, Ordering::Relaxed);
                }
            }
        }
        return ret;
    }
}

impl any_base::io::async_stream::Stream for Stream {
    fn raw_fd(&self) -> i32 {
        self.fd
    }
    fn is_sendfile(&self) -> bool {
        true
    }
}
impl any_base::io::async_stream::AsyncStream for Stream {
    fn poll_is_single(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
    }
    fn poll_raw_fd(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        return Poll::Ready(self.fd);
    }
}

impl any_base::io::async_read_msg::AsyncReadMsg for Stream {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        return Poll::Ready(Ok(StreamReadMsg::new()));
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        return Poll::Ready(Ok(StreamReadMsg::new()));
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<bool> {
        return Poll::Ready(false);
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

    fn is_read_msg(&self) -> bool {
        false
    }
    fn read_cache_size(&self) -> usize {
        0
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
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&self.s).poll_write_ready(cx)
    }

    fn poll_sendfile(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _file_fd: i32,
        _seek: u64,
        _size: usize,
    ) -> Poll<io::Result<usize>> {
        #[cfg(unix)]
        {
            use any_base::ready;
            ready!(Pin::new(&self.s).poll_write_ready(_cx))?;

            let mut sendfile = if self.sendfile_future.is_some() {
                self.sendfile_future.take().unwrap()
            } else {
                let socket_fd = self.fd;
                let _config = self._config.clone();
                let sendfile_zero_count = self.sendfile_zero_count.clone();
                Box::pin(async move {
                    Self::sendfile(
                        socket_fd,
                        _file_fd,
                        _seek,
                        _size,
                        &_config,
                        sendfile_zero_count,
                    )
                    .await
                })
            };

            let ret = Pin::new(&mut sendfile).poll(_cx);
            match ret {
                Poll::Ready(ret) => {
                    return Poll::Ready(ret);
                }
                Poll::Pending => {
                    self.sendfile_future = Some(sendfile);
                    return Poll::Pending;
                }
            }
        }
        #[cfg(not(unix))]
        {
            self.disable_warning();
            Poll::Ready(Ok(0))
        }
    }

    fn is_write_msg(&self) -> bool {
        false
    }
    fn write_cache_size(&self) -> usize {
        0
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
        let stream = Stream::new(stream, Arc::new(TcpConfig::default()));
        Ok((StreamFlow::new(stream, None), local_addr, remote_addr))
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

const PACK_MAX_NUM: i64 = 10000000;
#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    println!("pwd:{:?}", std::env::current_dir()?);
    if let Err(_) = log4rs::init_file("conf/log4rs.yaml", Default::default()) {
        if let Err(e) = log4rs::init_file("log4rs.yaml", Default::default()) {
            eprintln!("err:log4rs::init_file => e:{}", e);
            return Err(anyhow!("err:log4rs::init_file"))?;
        }
    }

    let connect_num = Arc::new(AtomicU64::new(0));
    let err_num = Arc::new(AtomicU64::new(0));
    let stream_close_num = Arc::new(AtomicU64::new(0));
    {
        let connect_num = connect_num.clone();
        let stream_close_num = stream_close_num.clone();
        let err_num = err_num.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                log::info!(
                    "tunnel server connect_num:{}, stream_close_num:{}, err_num:{}",
                    connect_num.load(Ordering::Relaxed),
                    stream_close_num.load(Ordering::Relaxed),
                    err_num.load(Ordering::Relaxed)
                );
            }
        });
    }
    loop {
        let wg = awaitgroup::WaitGroup::new();
        for _ in 0..10 {
            let connect_num = connect_num.clone();
            let err_num = err_num.clone();
            let stream_close_num = stream_close_num.clone();
            let cwi = wg.worker().add();
            tokio::spawn(async move {
                let ret: Result<()> = async {
                    let connect_addr = "127.0.0.1:28080".to_string();
                    log::info!("connect_addr:{}", connect_addr);
                    let client = client::Client::new();
                    let (mut stream, _, _) = client
                        .connect(
                            None,
                            Arc::new(Box::new(PeerStreamConnectTcp::new(connect_addr, 10, 0, 64))),
                            None,
                            ThreadRuntime,
                        )
                        .await?;
                    {
                        let connect_num = connect_num.fetch_add(1, Ordering::Relaxed) + 1;
                    }

                    {
                        let connect_num = connect_num.clone();
                        let stream_close_num = stream_close_num.clone();
                        let err_num = err_num.clone();

                        let stream = StreamFlow::new(stream, None);
                        let (mut r, mut w) = any_base::io::split::split(stream);

                        let wg = awaitgroup::WaitGroup::new();
                        let read_num = Arc::new(AtomicI64::new(0));
                        /*
                        {
                            let read_num = read_num.clone();
                            let connect_num = connect_num.clone();
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

                                        // if n % 1000 == 0 {
                                        //     tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                        // }

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

                         */
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
                                            w.shutdown().await?;
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
                                    "tunnel client connect_num:{}, stream_close_num:{}, err_num:{}, read_num:{}, write_num:{}",
                                    connect_num.load(Ordering::Relaxed),
                                    stream_close_num.load(Ordering::Relaxed),
                                    err_num.load(Ordering::Relaxed), read_num.load(Ordering::Relaxed), write_num.load(Ordering::Relaxed));
                                }
                            }
                        }
                        let stream_close_num = stream_close_num.fetch_add(1, Ordering::Relaxed) + 1;
                    }
                    return Ok(());
                }
                .await;
                if let Err(e) = ret {
                    log::error!("err:stream => e:{}", e);
                    err_num.fetch_add(1, Ordering::Relaxed);
                }
                cwi.done();
            });
        }
        wg.wait().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
