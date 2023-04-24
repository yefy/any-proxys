use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
//use std::future::Future;
use crate::io::async_read_msg::AsyncReadMsg;
use crate::io::async_stream::AsyncStream;
use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
use std::io;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
//use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub type StreamFlowRead = crate::io::split::ReadHalf<StreamFlow>;
pub type StreamFlowWrite = crate::io::split::WriteHalf<StreamFlow>;

//+ Sync
// pub trait AsyncReadAsyncWrite: AsyncRead + AsyncWrite + Send + Unpin {}
// impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadAsyncWrite for T {}

pub trait StreamRead: AsyncRead + AsyncReadMsg + AsyncStream + Send + Unpin {}
impl<T: AsyncRead + AsyncReadMsg + AsyncStream + Send + Unpin> StreamRead for T {}

pub trait StreamWrite: AsyncWrite + AsyncWriteMsg + AsyncStream + Send + Unpin {}
impl<T: AsyncWrite + AsyncWriteMsg + AsyncStream + Send + Unpin> StreamWrite for T {}

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
}

pub struct StreamFlow {
    read_timeout: tokio::time::Duration,
    write_timeout: tokio::time::Duration,
    info: Option<Arc<Mutex<StreamFlowInfo>>>,
    r: Box<dyn StreamRead + Send + Unpin>,
    w: Box<dyn StreamWrite + Send + Unpin>,
    fd: i32,
}

impl StreamFlow {
    pub fn split(self) -> (StreamFlowRead, StreamFlowWrite) {
        crate::io::split::split(self)
    }
    pub fn split_stream(
        self,
    ) -> (
        tokio::time::Duration,
        tokio::time::Duration,
        Option<Arc<Mutex<StreamFlowInfo>>>,
        Box<dyn StreamRead + Send + Unpin>,
        Box<dyn StreamWrite + Send + Unpin>,
        i32,
    ) {
        let StreamFlow {
            read_timeout,
            write_timeout,
            info,
            r,
            w,
            fd,
        } = self;
        return (read_timeout, write_timeout, info, r, w, fd);
    }

    pub fn new(
        fd: i32,
        r: Box<dyn StreamRead + Send + Unpin>,
        w: Box<dyn StreamWrite + Send + Unpin>,
    ) -> StreamFlow {
        StreamFlow {
            read_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            write_timeout: tokio::time::Duration::from_secs(std::u64::MAX),
            info: None,
            r,
            w,
            fd,
        }
    }
    pub fn fd(&self) -> i32 {
        self.fd
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
        info: Option<Arc<Mutex<StreamFlowInfo>>>,
    ) {
        self.read_timeout = read_timeout;
        self.write_timeout = write_timeout;
        self.set_stream_info(info)
    }

    pub fn set_stream_info(&mut self, mut info: Option<Arc<Mutex<StreamFlowInfo>>>) {
        if info.is_some() {
            let mut info = info.as_mut().unwrap().lock().unwrap();
            info.write_timeout = self.get_write_timeout();
            info.read_timeout = self.get_read_timeout();
        }
        self.info = info;
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

    fn read_flow(&mut self, ret: io::Result<usize>) -> io::Result<()> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let mut parse_err = |ret: io::Result<usize>| -> Result<usize> {
            match ret {
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
                    let mut info = self.info.as_ref().unwrap().lock().unwrap();
                    info.err = stream_err;
                    info.err_time_millis = Local::now().timestamp_millis();
                }
                log::debug!("read_flow kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(usize) => {
                if self.info.is_some() {
                    self.info.as_ref().unwrap().lock().unwrap().read += usize as i64;
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

    fn write_flow(&mut self, ret: io::Result<usize>, buffer_len: usize) -> io::Result<usize> {
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
                Err(e) => {
                    stream_err = StreamFlowErr::WriteErr;
                    kind = e.kind();
                    return Err(anyhow!("err:write_flow => kind:{:?}, e:{}", e.kind(), e));
                }
            }
        };

        let ret = parse_err(ret);

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
}

impl crate::io::async_stream::AsyncStream for StreamFlow {
    fn poll_is_single(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.r).poll_is_single(cx)
    }
}

impl crate::io::async_read_msg::AsyncReadMsg for StreamFlow {
    fn poll_read_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
        let ret = Pin::new(&mut self.r).poll_read_msg(cx);
        match ret {
            Poll::Ready(ret) => match ret {
                Err(e) => {
                    self.read_flow(io::Result::Err(e))?;
                    return Poll::Ready(Ok(Vec::new()));
                }
                Ok(data) => {
                    self.read_flow(io::Result::Ok(data.len()))?;
                    return Poll::Ready(Ok(data));
                }
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_is_read_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.r).poll_is_read_msg(cx)
    }
}

impl crate::io::async_write_msg::AsyncWriteMsg for StreamFlow {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        let ret = Pin::new(&mut self.w).poll_write_msg(cx, buf);
        match ret {
            Poll::Ready(ret) => Poll::Ready(self.write_flow(ret, buf.len())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_is_write_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.w).poll_is_write_msg(cx)
    }
}

impl tokio::io::AsyncRead for StreamFlow {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let ret = Pin::new(&mut self.r).poll_read(cx, buf);
        match ret {
            Poll::Ready(ret) => {
                let ret = if let Err(e) = ret {
                    self.read_flow(io::Result::Err(e))
                } else {
                    self.read_flow(io::Result::Ok(buf.filled().len()))
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
        let ret = Pin::new(&mut self.w).poll_write(cx, buf);
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

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.w).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.w).poll_shutdown(cx)
    }
}
