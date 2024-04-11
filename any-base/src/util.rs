use crate::io::async_write_msg::{MsgWriteBuf, MsgWriteBufFile};
use bytes::{Buf, Bytes};
#[cfg(unix)]
use libc::c_int;
use serde::de::{Deserialize, Deserializer};
use serde::ser::Serializer;
use serde::Serialize;
use std::collections::VecDeque;
use std::ops::Deref;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;
#[cfg(unix)]
use tokio::net::TcpStream;
//use libc::{c_int, setsockopt, getsockopt};
#[cfg(unix)]
use libc::c_void;
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::mem::{self, size_of, MaybeUninit};

#[cfg(unix)]
pub(crate) type Socket = c_int;

/// Helper macro to execute a system call that returns an `io::Result`.
#[cfg(unix)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        #[allow(unused_unsafe)]
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

// pub(crate) fn set_nonblocking(fd: Socket, nonblocking: bool) -> io::Result<()> {
//     if nonblocking {
//         fcntl_add(fd, libc::F_GETFL, libc::F_SETFL, libc::O_NONBLOCK)
//     } else {
//         fcntl_remove(fd, libc::F_GETFL, libc::F_SETFL, libc::O_NONBLOCK)
//     }
// }

/*
f let Some(interval) = keepalive.interval {
            let secs = into_secs(interval);
            unsafe { setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, secs)? }
        }

        if let Some(retries) = keepalive.retries {
            unsafe { setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_KEEPCNT, retries as c_int)? }
        }


pub fn recv_buffer_size(&self) -> io::Result<usize> {
    unsafe {
        _getsockopt::<c_int>(self.as_raw(), sys::SOL_SOCKET, sys::SO_RCVBUF)
            .map(|size| size as usize)
    }
}

/// Set value for the `SO_RCVBUF` option on this socket.
///
/// Changes the size of the operating system's receive buffer associated
/// with the socket.
pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
    unsafe {
        _setsockopt(
            self.as_raw(),
            sys::SOL_SOCKET,
            sys::SO_RCVBUF,
            size as c_int,
        )
    }
}
*/

#[cfg(unix)]
/// Add `flag` to the current set flags of `F_GETFD`.
fn fcntl_add(fd: Socket, get_cmd: c_int, set_cmd: c_int, flag: c_int) -> io::Result<()> {
    let previous = syscall!(fcntl(fd, get_cmd))?;
    let new = previous | flag;
    if new != previous {
        syscall!(fcntl(fd, set_cmd, new)).map(|_| ())
    } else {
        // Flag was already set.
        Ok(())
    }
}

#[cfg(unix)]
/// Remove `flag` to the current set flags of `F_GETFD`.
fn fcntl_remove(fd: Socket, get_cmd: c_int, set_cmd: c_int, flag: c_int) -> io::Result<()> {
    let previous = syscall!(fcntl(fd, get_cmd))?;
    let new = previous & !flag;
    if new != previous {
        syscall!(fcntl(fd, set_cmd, new)).map(|_| ())
    } else {
        // Flag was already set.
        Ok(())
    }
}

#[cfg(unix)]
/// Caller must ensure `T` is the correct type for `opt` and `val`.
pub(crate) unsafe fn _getsockopt<T>(fd: Socket, opt: c_int, val: c_int) -> io::Result<T> {
    let mut payload: MaybeUninit<T> = MaybeUninit::uninit();
    let mut len = size_of::<T>() as libc::socklen_t;
    syscall!(getsockopt(
        fd,
        opt,
        val,
        payload.as_mut_ptr().cast(),
        &mut len,
    ))
    .map(|_| {
        debug_assert_eq!(len as usize, size_of::<T>());
        // Safety: `getsockopt` initialised `payload` for us.
        payload.assume_init()
    })
}

#[cfg(unix)]
/// Caller must ensure `T` is the correct type for `opt` and `val`.
pub(crate) unsafe fn _setsockopt<T>(
    fd: Socket,
    opt: c_int,
    val: c_int,
    payload: T,
) -> io::Result<()> {
    let payload = &payload as *const T as *const c_void;
    syscall!(setsockopt(
        fd,
        opt,
        val,
        payload,
        mem::size_of::<T>() as libc::socklen_t,
    ))
    .map(|_| ())
}

#[cfg(unix)]
#[allow(dead_code)]
const O_DIRECT: libc::c_int = 040000; // Define O_DIRECT if it's not available in libc

#[cfg(unix)]
#[allow(dead_code)]
pub fn directio_on(fd: RawFd) -> anyhow::Result<()> {
    // let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // if flags == -1 {
    //     return Err(anyhow::anyhow!("err:directio_on libc::EBADF"));
    // }
    //
    // let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | O_DIRECT) };
    // if result == -1 {
    //     let c_err = unsafe { *libc::__errno_location() };
    //     use std::ffi::CStr;
    //     let error_message = unsafe { CStr::from_ptr(libc::strerror(c_err)) };
    //
    //     return Err(anyhow::anyhow!(
    //         "err:directio_on {}",
    //         error_message.to_string_lossy()
    //     ));
    // }
    // Ok(())

    let fd: Socket = fd;
    fcntl_add(fd, libc::F_GETFL, libc::F_SETFL, libc::O_DIRECT)?;
    Ok(())
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn directio_off(fd: RawFd) -> anyhow::Result<()> {
    // let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // if flags == -1 {
    //     return Err(anyhow::anyhow!("err:directio_off libc::EBADF"));
    // }
    //
    // let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags & !O_DIRECT) };
    // if result == -1 {
    //     let c_err = unsafe { *libc::__errno_location() };
    //     use std::ffi::CStr;
    //     let error_message = unsafe { CStr::from_ptr(libc::strerror(c_err)) };
    //
    //     return Err(anyhow::anyhow!(
    //         "err:directio_off {}",
    //         error_message.to_string_lossy()
    //     ));
    // }
    // Ok(())

    let fd: Socket = fd;
    fcntl_remove(fd, libc::F_GETFL, libc::F_SETFL, libc::O_DIRECT)?;
    Ok(())
}

#[cfg(unix)]
pub fn set_tcp_nopush2(stream: &TcpStream, enable: bool) {
    let fd = stream.as_raw_fd();
    set_tcp_nopush_(fd, enable)
}
#[cfg(unix)]
pub fn set_tcp_nopush_(fd: i32, enable: bool) {
    if enable {
        let enable = !enable;
        set_tcp_nodelay(fd, enable)
    }

    // use libc::{c_int, setsockopt, IPPROTO_TCP, TCP_CORK /*TCP_NOPUSH*/};
    //
    // let value: c_int = if enable { 1 } else { 0 };
    // let n = unsafe {
    //     setsockopt(
    //         fd,
    //         IPPROTO_TCP,
    //         TCP_CORK,
    //         &value as *const _ as *const _,
    //         std::mem::size_of_val(&value) as u32,
    //     )
    // };
    //
    // if n != 0 {
    //     log::error!("err:TCP_NOPUSH => TCP_NOPUSH:{}", enable);
    // }

    let fd: Socket = fd;
    let value: c_int = if enable { 1 } else { 0 };
    let ret = unsafe { _setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_CORK, value) };
    if let Err(_) = ret {
        log::error!("err:TCP_NOPUSH => TCP_NOPUSH:{}", enable);
    }
}
#[cfg(unix)]
pub fn set_tcp_nodelay2(stream: &TcpStream, enable: bool) {
    let fd = stream.as_raw_fd();
    set_tcp_nodelay(fd, enable)
}
#[cfg(unix)]
pub fn set_tcp_nodelay(fd: i32, enable: bool) {
    // use libc::{c_int, setsockopt, IPPROTO_TCP, TCP_NODELAY /*TCP_NOPUSH*/};
    //
    // let value: c_int = if enable { 1 } else { 0 };
    // let n = unsafe {
    //     setsockopt(
    //         fd,
    //         IPPROTO_TCP,
    //         TCP_NODELAY,
    //         &value as *const _ as *const _,
    //         std::mem::size_of_val(&value) as u32,
    //     )
    // };
    //
    //
    //
    // if n != 0 {
    //     log::error!("err:TCP_NODELAY => TCP_NODELAY:{}", enable);
    // }

    let fd: Socket = fd;
    let value: c_int = if enable { 1 } else { 0 };
    let ret = unsafe { _setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_NODELAY, value) };
    if let Err(_) = ret {
        log::error!("err:TCP_NODELAY => TCP_NODELAY:{}", enable);
    }
}

pub struct StreamReadMsg {
    pub datas: VecDeque<Bytes>,
    data_len: usize,
    file: Option<MsgWriteBufFile>,
    file_len: usize,
}

impl Default for StreamReadMsg {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamReadMsg {
    pub fn new() -> Self {
        StreamReadMsg {
            datas: VecDeque::with_capacity(10),
            data_len: 0,
            file: None,
            file_len: 0,
        }
    }

    pub fn is_file(&self) -> bool {
        self.file.is_some()
    }

    pub fn set_file(&mut self, file: MsgWriteBufFile) {
        self.file_len = file.remaining();
        self.file = Some(file);
    }

    pub fn take_file(&mut self) -> Option<MsgWriteBufFile> {
        self.file.take()
    }

    pub fn file_len(&self) -> usize {
        self.file_len
    }

    pub fn push_back_data(&mut self, data: Bytes) {
        self.data_len += data.len();
        self.datas.push_back(data);
    }

    pub fn push_back_msg(&mut self, data: MsgWriteBuf) {
        match data {
            MsgWriteBuf::Bytes(data) => self.push_back_data(data.to_bytes()),
            MsgWriteBuf::File(data) => {
                self.set_file(data);
            }
        }
    }

    pub fn take_data(self) -> VecDeque<Bytes> {
        self.datas
    }
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() <= 0
    }
    pub fn remaining(&self) -> usize {
        self.data_len() + self.file_len()
    }
    pub fn split(self) -> (VecDeque<Bytes>, Option<MsgWriteBufFile>) {
        (self.datas, self.file)
    }
}

#[derive(Clone)]
pub struct ArcString {
    str: Arc<String>,
}

impl From<&str> for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn from(s: &str) -> ArcString {
        ArcString::new(s.to_owned())
    }
}
impl Eq for ArcString {}

impl From<String> for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn from(s: String) -> ArcString {
        ArcString::new(s)
    }
}

impl Default for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn default() -> Self {
        ArcString::new("".to_string())
    }
}

impl ArcString {
    pub fn new(str: String) -> Self {
        ArcString { str: Arc::new(str) }
    }

    pub fn as_str(&self) -> &str {
        &self.str
    }

    pub fn string(&self) -> String {
        self.str.to_string()
    }
}

impl Deref for ArcString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.str
    }
}
use std::fmt;
impl fmt::Debug for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Drain").field(&self.as_str()).finish()
    }
}

impl fmt::Display for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.str, f)
    }
}

impl PartialEq for ArcString {
    fn eq(&self, other: &ArcString) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
    fn ne(&self, other: &ArcString) -> bool {
        PartialEq::ne(&self[..], &other[..])
    }
}

impl Serialize for ArcString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.str)
    }
}

impl<'de> Deserialize<'de> for ArcString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ArcString::new(s))
    }
}

pub fn bytes_index(data: &Bytes, pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

pub fn bytes_split(data: &Bytes, pattern: &[u8]) -> Vec<Bytes> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while let Some(mut pos) = bytes_index(&data.slice(start..), pattern) {
        pos += start; // Adjust position to global index
        chunks.push(data.slice(start..pos));
        start = pos + pattern.len(); // Skip pattern
    }

    if start < data.len() {
        chunks.push(data.slice(start..));
    }

    return chunks;
}

pub fn bytes_split_once(data: &Bytes, pattern: &[u8]) -> Option<(Bytes, Bytes)> {
    let index = bytes_index(data, pattern);
    if index.is_none() {
        return None;
    }
    let index = index.unwrap();
    return Some((data.slice(0..index), data.slice((index + pattern.len())..)));
}

pub fn buf_index(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

pub fn buf_split<'a>(data: &'a [u8], pattern: &[u8]) -> Vec<&'a [u8]> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while let Some(mut pos) = buf_index(&data[start..], pattern) {
        pos += start; // Adjust position to global index
        chunks.push(&data[start..pos]);
        start = pos + pattern.len(); // Skip pattern
    }

    if start < data.len() {
        chunks.push(&data[start..]);
    }

    return chunks;
}

pub fn buf_split_once<'a>(data: &'a [u8], pattern: &[u8]) -> Option<(&'a [u8], &'a [u8])> {
    let index = buf_index(data, pattern);
    if index.is_none() {
        return None;
    }
    let index = index.unwrap();
    return Some((&data[0..index], &data[(index + pattern.len())..]));
}

#[derive(Clone)]
pub struct HttpHeaderExt {
    //pub file_ext: ArcRwLock<HashMap<i32, ArcRwLock<FileExt>>>,
}

impl HttpHeaderExt {
    pub fn new() -> Self {
        HttpHeaderExt {}
    }
}
