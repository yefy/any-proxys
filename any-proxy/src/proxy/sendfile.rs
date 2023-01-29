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

        let ret: Result<u64> = tokio::task::spawn_blocking(move || {
            let wn = match unsafe { libc::sendfile(socket_fd, file_fd, &mut offset, count) } {
                -1 => return Err(anyhow!("err:libc::sendfile =>"))?,
                copied => copied as u64,
            };
            Ok(wn)
        })
        .await?;
        let ret: Result<u64> = async {
            match ret {
                Err(_) => {
                    stream_err = StreamFlowErr::WriteClose;
                    kind = io::ErrorKind::ConnectionReset;
                    return Err(anyhow!("err:sendfile close"));
                }
                Ok(size) => {
                    return Ok(size);
                }
            }
        }
        .await;

        match ret {
            Err(e) => {
                if self.info.is_some() {
                    self.info.as_ref().unwrap().lock().unwrap().err = stream_err;
                }
                log::debug!("write kind:{:?}, e:{:?}", kind, e);
                Err(std::io::Error::new(kind, e))
            }
            Ok(size) => {
                if self.info.is_some() {
                    self.info.as_ref().unwrap().lock().unwrap().write += size as i64;
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
