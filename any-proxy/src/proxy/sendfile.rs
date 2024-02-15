#![cfg(unix)]

use crate::config::config_toml::TcpConfig as Config;
use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use std::cmp::min;
use std::io;
use std::io::ErrorKind;
use std::sync::atomic::Ordering;

pub struct SendFile {
    info: ArcMutex<StreamFlowInfo>,
    fd: i32,
}

impl SendFile {
    pub fn new(fd: i32, info: ArcMutex<StreamFlowInfo>) -> SendFile {
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
            "libc::sendfile socket_fd:{}, file_fd:{}, offset:{}, count:{}",
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
                    if c_err == libc::EAGAIN || c_err == libc::EINTR || c_err == libc::EWOULDBLOCK {
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile WouldBlock"));
                    } else if c_err == libc::EINPROGRESS {
                        // if self.info.is_some() {
                        //         self.info.get_mut().err = StreamFlowErr::WriteClose;
                        // }
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile WouldBlock"));
                    } else if c_err == libc::ECONNRESET {
                        stream_err = StreamFlowErr::WriteClose;
                        kind = io::ErrorKind::ConnectionReset;
                        return Err(anyhow!("err:sendfile close"));
                    } else if c_err == 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile WouldBlock"));
                    }

                    stream_err = StreamFlowErr::WriteErr;
                    kind = io::ErrorKind::AddrNotAvailable;
                    return Err(anyhow!("err:sendfile AddrNotAvailable => c_err:{}", c_err));
                }
                copied => {
                    if copied == 0 {
                        stream_err = StreamFlowErr::Init;
                        kind = io::ErrorKind::WouldBlock;
                        return Err(anyhow!("err:sendfile WouldBlock"));
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
                    self.info.get_mut().err = stream_err;
                }
                //log::error!("write kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(size) => {
                log::trace!("sendfile write size:{:?}", size);
                if self.info.is_some() {
                    self.info.get_mut().write += size as i64;
                }
                Ok(size)
            }
        }
    }
}

pub async fn sendfile(
    socket_fd: i32,
    file_fd: i32,
    seek: u64,
    size: usize,
    config: &Config,
) -> io::Result<usize> {
    use crate::util::default_config::PAGE_SIZE;
    let page_size = PAGE_SIZE.load(Ordering::Relaxed);
    let sendfile_max_write_size = if config.sendfile_max_write_size > 0 {
        config.sendfile_max_write_size
    } else {
        usize::max_value() - page_size
    };
    let sendfile_max_write_size = min(size, sendfile_max_write_size);
    let sendfile_max_write_size = if sendfile_max_write_size <= page_size {
        sendfile_max_write_size
    } else {
        sendfile_max_write_size / page_size * page_size
    };
    let left_seek = page_size - (seek % page_size as u64) as usize;
    let size = if sendfile_max_write_size <= left_seek {
        sendfile_max_write_size
    } else {
        left_seek + (sendfile_max_write_size - left_seek) / page_size * page_size
    };

    let mut kind: ErrorKind = io::ErrorKind::NotFound;
    if socket_fd <= 0 || file_fd <= 0 {
        return Err(std::io::Error::new(
            kind,
            anyhow!("err:sendfile socket_fd <= 0 || file_fd <= 0"),
        ));
    }

    let mut offset: libc::off_t = seek as libc::off_t;
    let count: libc::size_t = size as libc::size_t;
    log::trace!(
        "libc::sendfile socket_fd:{}, file_fd:{}, offset:{}, count:{}",
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
    let ret: Result<usize> = async {
        match ret {
            -1 => {
                let c_err = unsafe { *libc::__errno_location() };
                use std::ffi::CStr;
                let error_message = unsafe { CStr::from_ptr(libc::strerror(c_err)) };
                log::trace!(
                    "sendfile c_err:{}, error_message:{:?}",
                    c_err,
                    error_message
                );
                if c_err == libc::EAGAIN || c_err == libc::EINTR || c_err == libc::EWOULDBLOCK {
                    if config.sendfile_eagain_sleep_mil_time > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            config.sendfile_eagain_sleep_mil_time,
                        ))
                        .await;
                    }
                    kind = io::ErrorKind::WouldBlock;
                    return Err(anyhow!("err:sendfile WouldBlock"));
                } else if c_err == libc::EINPROGRESS {
                    if config.sendfile_eagain_sleep_mil_time > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            config.sendfile_eagain_sleep_mil_time,
                        ))
                        .await;
                    }
                    kind = io::ErrorKind::WouldBlock;
                    return Err(anyhow!("err:sendfile WouldBlock"));
                } else if c_err == libc::ECONNRESET {
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:sendfile close"));
                } else if c_err == 0 {
                    if config.sendfile_eagain_sleep_mil_time > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            config.sendfile_eagain_sleep_mil_time,
                        ))
                        .await;
                    }
                    kind = io::ErrorKind::WouldBlock;
                    return Err(anyhow!("err:sendfile WouldBlock"));
                }

                kind = io::ErrorKind::AddrNotAvailable;
                return Err(anyhow!("err:sendfile AddrNotAvailable => c_err:{}", c_err));
            }
            copied => {
                if copied == 0 {
                    if config.sendfile_eagain_sleep_mil_time > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            config.sendfile_eagain_sleep_mil_time,
                        ))
                        .await;
                    }
                    kind = io::ErrorKind::WouldBlock;
                    return Err(anyhow!("err:sendfile WouldBlock"));
                }
                log::trace!("sendfile copied:{}", copied);
                return Ok(copied as usize);
            }
        };
    }
    .await;

    match ret {
        Err(e) => {
            log::trace!("sendfile write kind:{:?}, e:{:?}", kind, e);
            Err(std::io::Error::new(kind, e))
        }
        Ok(size) => {
            log::trace!("sendfile write size:{:?}", size);
            Ok(size)
        }
    }
}
