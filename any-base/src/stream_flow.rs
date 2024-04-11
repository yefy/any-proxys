use crate::io::async_read_msg::AsyncReadMsg;
use crate::io::async_stream::{AsyncStream, Stream};
use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
use crate::typ::{ArcMutex, ArcMutexWriteGuard};
use crate::util::StreamReadMsg;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use std::io;
use std::io::ErrorKind;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait StreamReadTokio: AsyncRead + Unpin {}
impl<T: AsyncRead + Unpin> StreamReadTokio for T {}

pub trait StreamWriteTokio: AsyncWrite + Unpin {}
impl<T: AsyncWrite + Unpin> StreamWriteTokio for T {}

pub trait StreamReadWriteTokio: StreamReadTokio + StreamWriteTokio + Unpin {}
impl<T: StreamReadTokio + StreamWriteTokio + Unpin> StreamReadWriteTokio for T {}

pub type StreamFlowRead = crate::io::split::ReadHalf<StreamFlow>;
pub type StreamFlowWrite = crate::io::split::WriteHalf<StreamFlow>;

pub trait StreamReadFlow: AsyncRead + AsyncReadMsg + AsyncStream + Stream + Unpin {}
impl<T: AsyncRead + AsyncReadMsg + AsyncStream + Stream + Unpin> StreamReadFlow for T {}

pub trait StreamWriteFlow: AsyncWrite + AsyncWriteMsg + AsyncStream + Stream + Unpin {}
impl<T: AsyncWrite + AsyncWriteMsg + AsyncStream + Stream + Unpin> StreamWriteFlow for T {}

pub trait StreamReadWriteFlow: StreamReadFlow + StreamWriteFlow + Unpin {}
impl<T: StreamReadFlow + StreamWriteFlow + Unpin> StreamReadWriteFlow for T {}

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
    pub write_timeout: u64,
    pub read_timeout: u64,
    pub err: StreamFlowErr,
    pub err_time_millis: i64,
}

impl StreamFlowInfo {
    pub fn new() -> StreamFlowInfo {
        StreamFlowInfo {
            write: 0,
            read: 0,
            write_timeout: 0,
            read_timeout: 0,
            err: StreamFlowErr::Init,
            err_time_millis: 0,
        }
    }

    pub fn reset_err(&mut self) {
        self.err = StreamFlowErr::Init;
        self.err_time_millis = 0;
    }

    pub fn is_close(&self) -> bool {
        if self.err == StreamFlowErr::WriteClose
            || self.err == StreamFlowErr::ReadClose
            || self.err == StreamFlowErr::WriteReset
            || self.err == StreamFlowErr::ReadReset
        {
            return true;
        }
        return false;
    }
}

pub struct StreamFlow {
    read_timeout: tokio::time::Duration,
    write_timeout: tokio::time::Duration,
    info: Option<ArcMutex<StreamFlowInfo>>,
    r: Box<dyn StreamReadFlow>,
    w: Box<dyn StreamWriteFlow>,
    w_err_to_close: Option<Vec<String>>,
}

unsafe impl Send for StreamFlow {}
unsafe impl Sync for StreamFlow {}

impl StreamFlow {
    pub fn info(&self) -> ArcMutexWriteGuard<StreamFlowInfo> {
        self.info.as_ref().unwrap().get_mut()
    }
    pub fn split(self) -> (StreamFlowRead, StreamFlowWrite) {
        crate::io::split::split(self)
    }
    pub fn split_stream(
        self,
    ) -> (
        tokio::time::Duration,
        tokio::time::Duration,
        Option<ArcMutex<StreamFlowInfo>>,
        Box<dyn StreamReadFlow>,
        Box<dyn StreamWriteFlow>,
        Option<Vec<String>>,
    ) {
        let StreamFlow {
            read_timeout,
            write_timeout,
            info,
            r,
            w,
            w_err_to_close,
        } = self;
        return (read_timeout, write_timeout, info, r, w, w_err_to_close);
    }

    pub fn new<RW: StreamReadWriteFlow + 'static>(
        rw: RW,
        w_err_to_close: Option<Vec<String>>,
    ) -> StreamFlow {
        let (r, w) = crate::io::split::split(rw);
        StreamFlow {
            read_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            write_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            info: None,
            r: Box::new(r),
            w: Box::new(w),
            w_err_to_close,
        }
    }

    pub fn new2<R: StreamReadFlow + 'static, W: StreamWriteFlow + 'static>(
        r: R,
        w: W,
        w_err_to_close: Option<Vec<String>>,
    ) -> StreamFlow {
        StreamFlow {
            read_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            write_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            info: None,
            r: Box::new(r),
            w: Box::new(w),
            w_err_to_close,
        }
    }

    pub fn new_tokio<RW: StreamReadWriteTokio + 'static>(
        rw: RW,
        w_err_to_close: Option<Vec<String>>,
    ) -> StreamFlow {
        let stream = super::stream_tokio::Stream::new(rw);
        let (r, w) = crate::io::split::split(stream);
        StreamFlow {
            read_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            write_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            info: None,
            r: Box::new(r),
            w: Box::new(w),
            w_err_to_close,
        }
    }
    pub fn get_read_timeout(&self) -> u64 {
        return self.read_timeout.as_secs();
    }
    pub fn get_write_timeout(&self) -> u64 {
        return self.write_timeout.as_secs();
    }
    pub fn set_config(
        &mut self,
        read_timeout: tokio::time::Duration,
        write_timeout: tokio::time::Duration,
        info: Option<ArcMutex<StreamFlowInfo>>,
    ) {
        self.read_timeout = read_timeout;
        self.write_timeout = write_timeout;
        self.set_stream_info(info)
    }

    pub fn set_stream_info(&mut self, info: Option<ArcMutex<StreamFlowInfo>>) {
        self.info = info;
        if self.info.is_some() {
            let mut info = self.info();
            info.write_timeout = self.get_write_timeout();
            info.read_timeout = self.get_read_timeout();
        }
    }

    /*
       async fn read_flow(&mut self, buf: &mut tokio::io::ReadBuf<'_>) -> io::Result<()> {
           let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
           let mut kind: ErrorKind = io::ErrorKind::NotFound;
           let ret: Result<usize> = async {
               match self.r.read(buf.initialize_unfilled()).await {
                   Ok(0) => {
                       stream_err = StreamFlowErr::ReadClose;
                       kind = io::ErrorKind::ConnectionReset;
                       return Err(anyhow!("err:read_flow close"));
                   }
                   Ok(usize) => {
                       return Ok(usize);
                   }
                   Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                       stream_err = StreamFlowErr::ReadTimeout;
                       kind = io::ErrorKind::TimedOut;
                       return Err(anyhow!("err:read_flow timeout"));
                   }
                   Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                       stream_err = StreamFlowErr::ReadReset;
                       kind = io::ErrorKind::ConnectionReset;
                       return Err(anyhow!("err:read_flow reset"));
                   }
                   Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                       stream_err = StreamFlowErr::ReadClose;
                       kind = io::ErrorKind::ConnectionReset;
                       return Err(anyhow!("err:read_flow close"));
                   }
                   Err(e) => {
                       stream_err = StreamFlowErr::ReadErr;
                       kind = e.kind();
                       return Err(anyhow!("err:read_flow => kind:{:?}, e:{}", e.kind(), e));
                   }
               }
           }
           .await;

           match ret {
               Err(e) => {
                   if self.info.is_some() {
                       let mut info = self.info.as_ref().unwrap().lock().unwrap();
                       info.err = stream_err;
                       info.err_time_millis = Local::now().timestamp_millis();
                   }
                   log::debug!("read_flow kind:{:?}, e:{:?}", kind, e);
                   Err(std::io::Error::new(kind, e))
               }
               Ok(usize) => {
                   buf.advance(usize);
                   if self.info.is_some() {
                       self.info.as_ref().unwrap().lock().unwrap().read += usize as i64;
                   }
                   Ok(())
               }
           }
       }

    */

    fn read_flow(&mut self, ret: io::Result<usize>, buffer_len: usize) -> io::Result<()> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let mut parse_err = |ret: io::Result<usize>| -> Result<usize> {
            match ret {
                Ok(0) => {
                    if buffer_len == 0 {
                        return Ok(0);
                    }
                    stream_err = StreamFlowErr::ReadClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:read_flow close"));
                }
                Ok(usize) => {
                    return Ok(usize);
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    stream_err = StreamFlowErr::ReadTimeout;
                    kind = io::ErrorKind::TimedOut;
                    return Err(anyhow!("err:read_flow timeout"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    stream_err = StreamFlowErr::ReadReset;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:read_flow reset"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                    stream_err = StreamFlowErr::ReadClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:read_flow close"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionAborted => {
                    stream_err = StreamFlowErr::ReadClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:read_flow close"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    stream_err = StreamFlowErr::ReadClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:read_flow close"));
                }
                Err(e) => {
                    stream_err = StreamFlowErr::ReadErr;
                    kind = e.kind();
                    return Err(anyhow!("err:read_flow => kind:{:?}, e:{}", e.kind(), e));
                }
            }
        };

        let ret = parse_err(ret);

        match ret {
            Err(e) => {
                if self.info.is_some() {
                    let mut info = self.info();
                    info.err = stream_err;
                    info.err_time_millis = Local::now().timestamp_millis();
                }
                log::debug!("read_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                if self.info.is_some() {
                    self.info().read += usize as i64;
                }
                Ok(())
            }
        }
    }

    /*
    async fn write_flow(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let ret: Result<usize> = async {
            match self.w.write(buf).await {
                Ok(0) => {
                    if buf.len() == 0 {
                        return Ok(0);
                    }
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Ok(usize) => {
                    return Ok(usize);
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    kind = io::ErrorKind::TimedOut;
                    return Err(anyhow!("err:write_flow timeout"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    stream_err = StreamFlowErr::WriteReset;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow reset"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Err(e) => {
                    stream_err = StreamFlowErr::WriteErr;
                    kind = e.kind();
                    return Err(anyhow!("err:write_flow => kind:{:?}, e:{}", e.kind(), e));
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if self.info.is_some() {
                    let mut info = self.info.as_ref().unwrap().lock().unwrap();
                    info.err = stream_err;
                    info.err_time_millis = Local::now().timestamp_millis();
                }
                log::debug!("write_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                if self.info.is_some() {
                    self.info.as_ref().unwrap().lock().unwrap().write += usize as i64;
                }
                Ok(usize)
            }
        }
    }

     */

    fn write_flow(&self, ret: io::Result<usize>, buffer_len: usize) -> io::Result<usize> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let mut parse_err = |ret: io::Result<usize>| -> Result<usize> {
            match ret {
                Ok(0) => {
                    if buffer_len == 0 {
                        return Ok(0);
                    }
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Ok(usize) => {
                    return Ok(usize);
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    stream_err = StreamFlowErr::WriteTimeout;
                    kind = io::ErrorKind::TimedOut;
                    return Err(anyhow!("err:write_flow timeout"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    stream_err = StreamFlowErr::WriteReset;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow reset"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotConnected => {
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionAborted => {
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => {
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:write_flow close"));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(0);
                }
                Err(e) => {
                    stream_err = StreamFlowErr::WriteErr;
                    kind = e.kind();
                    if self.w_err_to_close.is_some() {
                        let err_str = e.to_string();
                        for str in self.w_err_to_close.as_ref().unwrap() {
                            if err_str.contains(str) {
                                stream_err = StreamFlowErr::WriteClose;
                                kind = io::ErrorKind::ConnectionReset;
                                return Err(anyhow!("err:write_flow close"));
                            }
                        }
                    }
                    return Err(anyhow!("err:write_flow => kind:{:?}, e:{}", e.kind(), e));
                }
            }
        };

        let ret = parse_err(ret);

        match ret {
            Err(e) => {
                if self.info.is_some() {
                    let mut info = self.info();
                    info.err = stream_err;
                    info.err_time_millis = Local::now().timestamp_millis();
                }
                log::debug!("write_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                if self.info.is_some() {
                    self.info().write += usize as i64;
                }
                Ok(usize)
            }
        }
    }
}
impl crate::io::async_stream::Stream for StreamFlow {
    fn raw_fd(&self) -> i32 {
        self.r.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        self.r.is_sendfile()
    }
}
impl crate::io::async_stream::AsyncStream for StreamFlow {
    fn poll_is_single(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut *self.r).poll_is_single(cx)
    }
    fn poll_raw_fd(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        Pin::new(&mut *self.r).poll_raw_fd(cx)
    }
}

impl crate::io::async_read_msg::AsyncReadMsg for StreamFlow {
    fn poll_try_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let ret = Pin::new(&mut *self.r).poll_try_read_msg(cx, msg_size);
        match ret {
            Poll::Ready(ret) => match ret {
                Err(e) => {
                    self.read_flow(io::Result::Err(e), 0)?;
                    return Poll::Ready(Ok(StreamReadMsg::new()));
                }
                Ok(data) => {
                    self.read_flow(io::Result::Ok(data.remaining()), 0)?;
                    return Poll::Ready(Ok(data));
                }
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_read_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let ret = Pin::new(&mut *self.r).poll_read_msg(cx, msg_size);
        match ret {
            Poll::Ready(ret) => match ret {
                Err(e) => {
                    self.read_flow(io::Result::Err(e), 1)?;
                    return Poll::Ready(Ok(StreamReadMsg::new()));
                }
                Ok(data) => {
                    self.read_flow(io::Result::Ok(data.remaining()), 1)?;
                    return Poll::Ready(Ok(data));
                }
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_is_read_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut *self.r).poll_is_read_msg(cx)
    }
    fn poll_try_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let ret = Pin::new(&mut *self.r).poll_try_read(cx, buf);
        match ret {
            Poll::Ready(ret) => {
                let ret = if let Err(e) = ret {
                    self.read_flow(io::Result::Err(e), 0)
                } else {
                    self.read_flow(io::Result::Ok(buf.filled().len()), 0)
                };
                Poll::Ready(ret)
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn is_read_msg(&self) -> bool {
        self.r.is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        self.r.read_cache_size()
    }
}

impl crate::io::async_write_msg::AsyncWriteMsg for StreamFlow {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        let ret = Pin::new(&mut *self.w).poll_write_msg(cx, buf);
        match ret {
            Poll::Ready(ret) => Poll::Ready(self.write_flow(ret, buf.len())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_is_write_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut *self.w).poll_is_write_msg(cx)
    }
    fn poll_write_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let ret = Pin::new(&mut *self.w).poll_write_ready(cx);
        match ret {
            Poll::Ready(ret) => match ret {
                Err(e) => {
                    let ret = self.write_flow(io::Result::Err(e), 0);
                    match ret {
                        Err(e) => Poll::Ready(io::Result::Err(e)),
                        Ok(_) => Poll::Ready(Ok(())),
                    }
                }
                Ok(()) => {
                    let ret = self.write_flow(io::Result::Ok(0), 0);
                    match ret {
                        Err(e) => Poll::Ready(io::Result::Err(e)),
                        Ok(_) => Poll::Ready(Ok(())),
                    }
                }
            },
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_sendfile(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        let ret = Pin::new(&mut *self.w).poll_sendfile(cx, file_fd, seek, size);
        match ret {
            Poll::Ready(ret) => Poll::Ready(self.write_flow(ret, size as usize)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_write_msg(&self) -> bool {
        self.w.is_write_msg()
    }
    fn write_cache_size(&self) -> usize {
        self.w.write_cache_size()
    }
}

impl tokio::io::AsyncRead for StreamFlow {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let ret = Pin::new(&mut *self.r).poll_read(cx, buf);
        match ret {
            Poll::Ready(ret) => {
                let ret = if let Err(e) = ret {
                    self.read_flow(io::Result::Err(e), 1)
                } else {
                    self.read_flow(io::Result::Ok(buf.filled().len()), 1)
                };
                Poll::Ready(ret)
            }
            Poll::Pending => Poll::Pending,
        }
        // let mut read_fut = Box::pin(self.read_flow(buf));
        // let ret = read_fut.as_mut().poll(cx);
        // match ret {
        //     Poll::Ready(ret) => Poll::Ready(ret),
        //     Poll::Pending => Poll::Pending,
        // }
    }
}

impl tokio::io::AsyncWrite for StreamFlow {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let ret = Pin::new(&mut *self.w).poll_write(cx, buf);
        match ret {
            Poll::Ready(ret) => Poll::Ready(self.write_flow(ret, buf.len())),
            Poll::Pending => Poll::Pending,
        }
        // let mut write_fut = Box::pin(self.write_flow(buf));
        // let ret = write_fut.as_mut().poll(cx);
        // match ret {
        //     Poll::Ready(ret) => Poll::Ready(ret),
        //     Poll::Pending => Poll::Pending,
        // }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let ret = Pin::new(&mut *self.w).poll_write_vectored(cx, bufs);
        match ret {
            Poll::Ready(ret) => Poll::Ready(self.write_flow(ret, bufs.len())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.w.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.w).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.w).poll_shutdown(cx)
    }
}
