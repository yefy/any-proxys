#![cfg(unix)]

use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use anyhow::anyhow;
use anyhow::Result;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::Mutex;

pub struct SendFile {
    info: Option<Arc<Mutex<StreamFlowInfo>>>,
    fd: i32,
}

impl SendFile {
    pub fn new(fd: i32, info: Option<Arc<Mutex<StreamFlowInfo>>>) -> SendFile {
        SendFile { fd, info }
    }
    pub async fn write(&self, fd: i32, seek: u64, size: u64) -> io::Result<u64> {
        let mut stream_err: StreamFlowErr = StreamFlowErr::Init;
        let mut kind: ErrorKind = io::ErrorKind::NotFound;
        let socket_fd = self.fd;
        let file_fd = fd;
        let mut offset: libc::off_t = seek as libc::off_t;
        let count: libc::size_t = size as libc::size_t;
        log::trace!(
            "libc::sendfile socket_fd:{}, socket_fd:{}, offset:{}, count:{}",
            socket_fd,
            file_fd,
            offset,
            count
        );

        let ret: isize = tokio::task::spawn_blocking(move || {
            let wn = unsafe { libc::sendfile(socket_fd, file_fd, &mut offset, count) };
            wn
        })
        .await?;
        let ret: Result<u64> = async {
            match ret {
                -1 => {
                    let c_err = unsafe { *libc::__errno_location() };
                    log::trace!("sendfile c_err:{}", c_err);
                    if c_err == libc::EAGAIN {
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile EAGAIN"));
                    }

                    if c_err == libc::EINPROGRESS {
                        if self.info.is_some() {
                            {
                                self.info.as_ref().unwrap().lock().unwrap().err =
                                    StreamFlowErr::WriteClose;
                            }
                        }
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile EAGAIN"));
                    }

                    if c_err == libc::ECONNRESET {
                        stream_err = StreamFlowErr::WriteClose;
                        kind = io::ErrorKind::ConnectionReset;
                        return Err(anyhow!("err:sendfile close"));
                    }

                    stream_err = StreamFlowErr::WriteErr;
                    kind = io::ErrorKind::AddrNotAvailable;
                    return Err(anyhow!("err:sendfile AddrNotAvailable => c_err:{}", c_err));
                }
                copied => {
                    if copied == 0 {
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile EAGAIN"));
                    }
                    log::trace!("sendfile copied:{}", copied);
                    return Ok(copied as u64);
                }
            };
        }
        .await;

        match ret {
            Err(e) => {
                if self.info.is_some() && stream_err != StreamFlowErr::Init {
                    {
                        self.info.as_ref().unwrap().lock().unwrap().err = stream_err;
                    }
                }
                //log::error!("write kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(size) => {
                log::trace!("sendfile write size:{:?}", size);
                if self.info.is_some() {
                    {
                        self.info.as_ref().unwrap().lock().unwrap().write += size as i64;
                    }
                }
                Ok(size)
            }
        }
    }

    pub async fn write_all(&self, fd: i32, mut seek: u64, mut size: u64) -> io::Result<()> {
        loop {
            let n = self.write(fd, seek, size).await?;
            seek += n;
            size -= n;
            if size <= 0 {
                return Ok(());
            }
        }
    }
}
