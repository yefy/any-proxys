use chrono::prelude::*;
use std::future::Future;
use std::io;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

//+ Sync
pub trait AsyncReadAsyncWrite: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadAsyncWrite for T {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFlowErr {
    WriteClose = 1,
    ReadClose = 2,
    WriteReset = 3,
    ReadReset = 4,
    WriteTimeout = 5,
    ReadTimeout = 6,
    WriteErr = 7,
    ReadErr = 8,
    Init = 9,
}

pub struct StreamFlowInfo {
    pub write: i64,
    pub read: i64,
    pub err: StreamFlowErr,
}

impl StreamFlowInfo {
    pub fn new() -> StreamFlowInfo {
        StreamFlowInfo {
            write: 0,
            read: 0,
            err: StreamFlowErr::Init,
        }
    }
}

pub struct StreamFlow {
    read_timeout: tokio::time::Duration,
    write_timeout: tokio::time::Duration,
    stream_info: Option<Arc<Mutex<StreamFlowInfo>>>,
    stream: Mutex<Box<dyn AsyncReadAsyncWrite>>,
    is_rw_timeout: AtomicBool,
    rw_time_mil: AtomicI64,
}

impl StreamFlow {
    pub fn new(stream: Box<dyn AsyncReadAsyncWrite>) -> StreamFlow {
        StreamFlow {
            read_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            write_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            stream_info: None,
            stream: Mutex::new(stream),
            is_rw_timeout: AtomicBool::new(true),
            rw_time_mil: AtomicI64::new(Local::now().timestamp_millis()),
        }
    }
    pub fn set_config(
        &mut self,
        read_timeout: tokio::time::Duration,
        write_timeout: tokio::time::Duration,
        is_rw_timeout: bool,
        stream_info: Option<Arc<Mutex<StreamFlowInfo>>>,
    ) {
        self.read_timeout = read_timeout;
        self.write_timeout = write_timeout;
        self.is_rw_timeout = AtomicBool::new(is_rw_timeout);
        self.rw_time_mil = AtomicI64::new(Local::now().timestamp_millis());
        self.stream_info = stream_info;
    }

    pub fn set_stream_info(&mut self, stream_info: Option<Arc<Mutex<StreamFlowInfo>>>) {
        self.stream_info = stream_info;
    }

    async fn read_flow(&self, buf: &mut tokio::io::ReadBuf<'_>) -> io::Result<()> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let ret: anyhow::Result<usize> = async {
            loop {
                match tokio::time::timeout(
                    self.read_timeout,
                    self.stream.lock().unwrap().read(buf.initialize_unfilled()),
                )
                .await
                {
                    Ok(ret) => match ret {
                        Ok(0) => {
                            stream_err = StreamFlowErr::ReadClose;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:read_flow close"));
                        }
                        Ok(usize) => {
                            if self.is_rw_timeout.load(Ordering::Relaxed) {
                                self.rw_time_mil
                                    .swap(Local::now().timestamp_millis(), Ordering::Relaxed);
                            }
                            return Ok(usize);
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                            stream_err = StreamFlowErr::ReadTimeout;
                            kind = io::ErrorKind::TimedOut;
                            return Err(anyhow::anyhow!("err:read_flow timeout"));
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                            stream_err = StreamFlowErr::ReadReset;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:read_flow reset"));
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                            stream_err = StreamFlowErr::ReadClose;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:read_flow close"));
                        }
                        Err(e) => {
                            stream_err = StreamFlowErr::ReadErr;
                            kind = e.kind();
                            return Err(anyhow::anyhow!(
                                "err:read_flow => kind:{:?}, e:{}",
                                e.kind(),
                                e
                            ));
                        }
                    },
                    Err(_) => {
                        if self.is_rw_timeout.load(Ordering::Relaxed) {
                            let rw_time_mil = self.rw_time_mil.load(Ordering::Relaxed);
                            if Local::now().timestamp_millis() - rw_time_mil
                                < self.read_timeout.as_millis() as i64
                            {
                                continue;
                            }
                        }
                        stream_err = StreamFlowErr::ReadTimeout;
                        kind = io::ErrorKind::TimedOut;
                        return Err(anyhow::anyhow!("err:read_flow timeout"));
                    }
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if self.stream_info.is_some() {
                    self.stream_info.as_ref().unwrap().lock().unwrap().err = stream_err;
                }
                log::debug!("read_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                buf.advance(usize);
                if self.stream_info.is_some() {
                    self.stream_info.as_ref().unwrap().lock().unwrap().read += usize as i64;
                }
                Ok(())
            }
        }
    }

    async fn write_flow(&self, buf: &[u8]) -> io::Result<usize> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let ret: anyhow::Result<usize> = async {
            loop {
                match tokio::time::timeout(
                    self.write_timeout,
                    self.stream.lock().unwrap().write(buf),
                )
                .await
                {
                    Ok(ret) => match ret {
                        Ok(0) => {
                            stream_err = StreamFlowErr::WriteClose;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:write_flow close"));
                        }
                        Ok(usize) => {
                            if self.is_rw_timeout.load(Ordering::Relaxed) {
                                self.rw_time_mil
                                    .swap(Local::now().timestamp_millis(), Ordering::Relaxed);
                            }
                            return Ok(usize);
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                            stream_err = StreamFlowErr::WriteTimeout;
                            kind = io::ErrorKind::TimedOut;
                            return Err(anyhow::anyhow!("err:write_flow timeout"));
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                            stream_err = StreamFlowErr::WriteReset;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:write_flow reset"));
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                            stream_err = StreamFlowErr::WriteClose;
                            kind = io::ErrorKind::ConnectionReset;
                            return Err(anyhow::anyhow!("err:write_flow close"));
                        }
                        Err(e) => {
                            stream_err = StreamFlowErr::WriteErr;
                            kind = e.kind();
                            return Err(anyhow::anyhow!(
                                "err:write_flow => kind:{:?}, e:{}",
                                e.kind(),
                                e
                            ));
                        }
                    },
                    Err(_) => {
                        if self.is_rw_timeout.load(Ordering::Relaxed) {
                            let rw_time_mil = self.rw_time_mil.load(Ordering::Relaxed);
                            if Local::now().timestamp_millis() - rw_time_mil
                                < self.write_timeout.as_millis() as i64
                            {
                                continue;
                            }
                        }
                        stream_err = StreamFlowErr::WriteTimeout;
                        kind = io::ErrorKind::TimedOut;
                        return Err(anyhow::anyhow!("err:write_flow timeout"));
                    }
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if self.stream_info.is_some() {
                    self.stream_info.as_ref().unwrap().lock().unwrap().err = stream_err;
                }
                log::debug!("write_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                if self.stream_info.is_some() {
                    self.stream_info.as_ref().unwrap().lock().unwrap().write += usize as i64;
                }
                Ok(usize)
            }
        }
    }
}

impl tokio::io::AsyncRead for StreamFlow {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncRead for &StreamFlow {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut read_fut = Box::pin(self.read_flow(buf));
        let ret = read_fut.as_mut().poll(cx);
        match ret {
            Poll::Ready(ret) => Poll::Ready(ret),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl tokio::io::AsyncWrite for StreamFlow {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self).poll_shutdown(cx)
    }
}

impl tokio::io::AsyncWrite for &StreamFlow {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut write_fut = Box::pin(self.write_flow(buf));
        let ret = write_fut.as_mut().poll(cx);
        match ret {
            Poll::Ready(ret) => Poll::Ready(ret),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream.lock().unwrap()).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream.lock().unwrap()).poll_shutdown(cx)
    }
}
