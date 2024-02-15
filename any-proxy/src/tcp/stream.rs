use crate::config::config_toml::TcpConfig as Config;
#[cfg(unix)]
use crate::proxy::sendfile;
use any_base::io::async_write_msg::AsyncWriteBuf;
use any_base::util::StreamReadMsg;
#[cfg(unix)]
use std::future::Future;
use std::io;
use std::pin::Pin;
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

pub struct Stream {
    s: TcpStream,
    fd: i32,
    _config: Arc<Config>,
    #[cfg(unix)]
    sendfile_future:
        Option<Pin<Box<dyn Future<Output = io::Result<usize>> + std::marker::Send + Sync>>>,
    #[cfg(unix)]
    sendfile_zero_count: Arc<AtomicUsize>,
}

impl Stream {
    pub fn new(s: TcpStream, _config: Arc<Config>) -> Stream {
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
        _config: &Config,
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
