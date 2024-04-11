use crate::io::is_write_msg::{is_write_msg, IsWriteMsg};
use crate::io::sendfile::{sendfile, Sendfile};
use crate::io::writable::{writable, Writable};
use crate::io::write_msg::{write_msg, WriteMsg};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{self};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Writes bytes asynchronously.
///
/// The trait inherits from [`std::io::Write`] and indicates that an I/O object is
/// **nonblocking**. All non-blocking I/O objects must return an error when
/// bytes cannot be written instead of blocking the current thread.
///
/// Specifically, this means that the [`poll_write`] function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(n))` means that `n` bytes of data was immediately
///   written.
///
/// * `Poll::Pending` means that no data was written from the buffer
///   provided. The I/O object is not currently writable but may become writable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is writable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   writable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the [`write`][stdwrite] method only works in
/// the context of a future's task. The object may panic if used outside of a task.
///
/// Note that this trait also represents that the  [`Write::flush`][stdflush] method
/// works very similarly to the `write` method, notably that `Ok(())` means that the
/// writer has successfully been flushed, a "would block" error means that the
/// current task is ready to receive a notification when flushing can make more
/// progress, and otherwise normal errors can happen as well.
///
/// Utilities for working with `AsyncWrite` values are provided by
/// [`AsyncWriteExt`].
///
/// [`std::io::Write`]: std::io::Write
/// [`poll_write`]: AsyncWrite::poll_write()
/// [stdwrite]: std::io::Write::write()
/// [stdflush]: std::io::Write::flush()
/// [`AsyncWriteExt`]: crate::io::AsyncWriteExt

const SENDFILE_DATA_FLAG: &'static [u8] = b"$@sf@$";
const SENDFILE_DATA_LEN: usize = SENDFILE_DATA_FLAG.len() + 4 + 8 + 8;

pub trait BufFile {
    fn is_file(&self) -> bool;

    fn remaining_(&self) -> usize;

    fn advance_(&mut self, cnt: usize);

    fn has_remaining_(&self) -> bool {
        self.remaining_() > 0
    }
}

#[derive(Clone)]
pub enum MsgWriteBuf {
    Bytes(MsgWriteBufBytes),
    File(MsgWriteBufFile),
}

impl MsgWriteBuf {
    pub fn from_bytes(data: Bytes) -> Self {
        MsgWriteBuf::Bytes(MsgWriteBufBytes::from_bytes(data))
    }

    pub fn from_vec(data: Vec<u8>) -> Self {
        Self::from_bytes(Bytes::from(data))
    }

    pub fn from_file(
        file_ext: Arc<FileExt>,
        seek: u64,
        size: usize,
        notify_tx: Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
    ) -> Self {
        MsgWriteBuf::File(MsgWriteBufFile::new(file_ext, seek, size, notify_tx))
    }

    pub fn to_bytes(self) -> Option<Bytes> {
        if let MsgWriteBuf::Bytes(data) = self {
            Some(data.data)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.remaining()
    }
}

impl Buf for MsgWriteBuf {
    #[inline]
    fn remaining(&self) -> usize {
        match self {
            Self::Bytes(d) => d.remaining(),
            Self::File(d) => d.remaining(),
        }
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        match self {
            Self::Bytes(d) => d.chunk(),
            Self::File(d) => d.chunk(),
        }
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        match self {
            Self::Bytes(d) => d.advance(cnt),
            Self::File(d) => d.advance(cnt),
        }
    }

    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        match self {
            Self::Bytes(d) => d.copy_to_bytes(len),
            Self::File(d) => d.copy_to_bytes(len),
        }
    }
}
use crate::file_ext::FileExt;
use std::cmp::min;
use std::fmt;
use std::sync::Arc;

impl fmt::Debug for MsgWriteBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[derive(Debug)]
        struct Streaming;
        #[derive(Debug)]
        struct Empty;
        #[derive(Debug)]
        struct Full<'a>(&'a Bytes);

        let mut builder = f.debug_tuple("MsgWriteBuf");
        builder.finish()
    }
}

impl fmt::Display for MsgWriteBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MsgWriteBuf")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct MsgReadBufFile {
    pub file_ext: Arc<FileExt>,
    pub seek: u64,
    pub size: u64,
}

impl MsgReadBufFile {
    pub fn default() -> MsgReadBufFile {
        MsgReadBufFile {
            file_ext: FileExt::default_arc(),
            seek: 0,
            size: 0,
        }
    }
    pub fn new(file_ext: Arc<FileExt>, seek: u64, size: u64) -> MsgReadBufFile {
        MsgReadBufFile {
            file_ext,
            seek,
            size,
        }
    }

    pub fn to_msg_write_buf_file(&self) -> (usize, MsgWriteBufFile) {
        let size = min(self.size, usize::max_value() as u64);
        let size = size as usize;
        (
            size,
            MsgWriteBufFile::new(self.file_ext.clone(), self.seek, size, Some(Vec::new())),
        )
    }

    pub fn to_msg_write_buf(&self) -> (usize, MsgWriteBuf) {
        let (size, file) = self.to_msg_write_buf_file();
        (size, MsgWriteBuf::File(file))
    }

    pub fn get(&self) -> (Arc<FileExt>, u64, usize) {
        let size = min(self.size, usize::max_value() as u64);
        (self.file_ext.clone(), self.seek, size as usize)
    }

    // pub fn to_msg_write_buf_file2(&self, size: usize) -> (usize, MsgWriteBufFile) {
    //     let size = if size <= 0 { usize::max_value() } else { size };
    //
    //     let size = min(self.size, size as u64);
    //     let size = size as usize;
    //     (
    //         size,
    //         MsgWriteBufFile::new(self.file_ext.clone(), self.file_fd, self.seek, size),
    //     )
    // }

    // pub fn to_msg_write_buf2(&self, size: usize) -> (usize, MsgWriteBuf) {
    //     let size = if size <= 0 { usize::max_value() } else { size };
    //     let (size, file) = self.to_msg_write_buf_file2(size);
    //     (size, MsgWriteBuf::File(file))
    // }

    pub fn get2(&self, size: usize) -> (Arc<FileExt>, u64, usize) {
        let size = if size <= 0 { usize::max_value() } else { size };

        let size = min(self.size, size as u64);
        (self.file_ext.clone(), self.seek, size as usize)
    }

    pub fn remaining(&self) -> u64 {
        self.size
    }

    pub fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    pub fn advance(&mut self, cnt: usize) {
        self.seek += cnt as u64;
        self.size -= cnt as u64;
    }

    pub fn from_slice(data: &[u8]) -> Option<MsgReadBufFile> {
        if data.len() != SENDFILE_DATA_LEN {
            return None;
        }

        if &data[0..SENDFILE_DATA_FLAG.len()] != SENDFILE_DATA_FLAG {
            return None;
        }
        let mut value = &data[SENDFILE_DATA_FLAG.len()..];
        let file_fd = value.get_i32();
        let seek = value.get_u64();
        let size = value.get_u64();

        Some(MsgReadBufFile {
            file_ext: FileExt::new_fd_arc(file_fd),
            seek,
            size,
        })
    }

    pub fn from_bytes(data: Bytes) -> Option<MsgReadBufFile> {
        Self::from_slice(data.as_ref())
    }
}

#[derive(Clone)]
pub struct MsgWriteBufFile {
    pub file_ext: Arc<FileExt>,
    pub seek: u64,
    pub size: usize,
    pub data: Bytes,
    pub notify_tx: Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl MsgWriteBufFile {
    pub fn new(
        file_ext: Arc<FileExt>,
        seek: u64,
        size: usize,
        notify_tx: Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
    ) -> MsgWriteBufFile {
        MsgWriteBufFile {
            file_ext: file_ext.clone(),
            seek,
            size,
            data: Self::data_to_bytes(file_ext.fix.file_fd, seek, size),
            notify_tx,
        }
    }

    // pub fn new_data(
    //     file_ext: Arc<FileExt>,
    //     file_fd: i32,
    //     seek: u64,
    //     size: usize,
    //     data: Option<Bytes>,
    //     notify_tx: Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
    // ) -> MsgWriteBufFile {
    //     if data.is_none() {
    //         return MsgWriteBufFile::new(file_ext, file_fd, seek, size);
    //     }
    //     let data = data.unwrap();
    //     MsgWriteBufFile {
    //         file_ext,
    //         file_fd,
    //         seek,
    //         size,
    //         data,
    //         notify_tx,
    //     }
    // }

    pub fn data_to_bytes(file_fd: i32, seek: u64, size: usize) -> Bytes {
        let mut buf = BytesMut::with_capacity(SENDFILE_DATA_LEN);
        buf.put_slice(SENDFILE_DATA_FLAG);
        buf.put_i32(file_fd);
        buf.put_u64(seek);
        buf.put_u64(size as u64);
        buf.freeze()
    }

    pub fn split(
        self,
    ) -> (
        Arc<FileExt>,
        u64,
        usize,
        Bytes,
        Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
    ) {
        let MsgWriteBufFile {
            file_ext,
            seek,
            size,
            data,
            notify_tx,
        } = self;
        return (file_ext, seek, size, data, notify_tx);
    }
}

impl BufFile for MsgWriteBufFile {
    fn is_file(&self) -> bool {
        true
    }

    fn remaining_(&self) -> usize {
        self.remaining()
    }

    fn advance_(&mut self, cnt: usize) {
        self.advance(cnt)
    }

    fn has_remaining_(&self) -> bool {
        self.remaining_() > 0
    }
}

impl Buf for MsgWriteBufFile {
    #[inline]
    fn remaining(&self) -> usize {
        self.size as usize
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.data.as_ref()
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.remaining(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.remaining(),
        );

        self.seek += cnt as u64;
        self.size -= cnt;
        if self.size == 0 {
            let notify_tx = self.notify_tx.take();
            if notify_tx.is_some() {
                for tx in notify_tx.unwrap() {
                    let _ = tx.send(());
                }
            }
        }
    }

    fn copy_to_bytes(&mut self, _len: usize) -> Bytes {
        panic!("not copy_to_bytes")
    }
}

#[derive(Clone, Debug)]
pub struct MsgWriteBufBytes {
    pub data: Bytes,
}

impl MsgWriteBufBytes {
    pub fn from_len(len: usize) -> MsgWriteBufBytes {
        let data = format!("{:X}\r\n", len);
        MsgWriteBufBytes {
            data: Bytes::from(data),
        }
    }

    pub fn from_bytes(data: Bytes) -> Self {
        MsgWriteBufBytes { data }
    }

    pub fn to_bytes(self) -> Bytes {
        self.data
    }
}

impl BufFile for MsgWriteBufBytes {
    fn is_file(&self) -> bool {
        true
    }

    fn remaining_(&self) -> usize {
        self.remaining()
    }

    fn advance_(&mut self, cnt: usize) {
        self.advance(cnt)
    }

    fn has_remaining_(&self) -> bool {
        self.remaining_() > 0
    }
}

impl Buf for MsgWriteBufBytes {
    #[inline]
    fn remaining(&self) -> usize {
        self.data.len()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.data.chunk()
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        self.data.advance(cnt)
    }

    fn copy_to_bytes(&mut self, _len: usize) -> Bytes {
        panic!("not copy_to_bytes")
    }
}

pub struct AsyncWriteBuf {
    data: MsgWriteBuf,
}

impl AsyncWriteBuf {
    pub fn new(data: MsgWriteBuf) -> AsyncWriteBuf {
        AsyncWriteBuf { data }
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn data(&mut self) -> MsgWriteBuf {
        self.data.clone()
    }
}

pub trait AsyncWriteMsg {
    /// Attempt to write bytes from `buf` into the object.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`. If successful,
    /// then it must be guaranteed that `n <= buf.len()`. A return value of `0`
    /// typically means that the underlying object is no longer able to accept
    /// bytes and will likely not be able to in the future as well, or that the
    /// buffer provided is empty.
    ///
    /// If the object is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object becomes
    /// writable or is closed.
    fn poll_write_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<Result<usize, io::Error>>;
    fn is_write_msg(&self) -> bool;
    fn write_cache_size(&self) -> usize;
    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool>;
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>>;
    fn poll_sendfile(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>>;
}

macro_rules! deref_async_write_msg {
    () => {
        fn poll_write_msg(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut AsyncWriteBuf,
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut **self).poll_write_msg(cx, buf)
        }
        fn poll_is_write_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
            Pin::new(&mut **self).poll_is_write_msg(cx)
        }
        fn poll_write_ready(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut **self).poll_write_ready(cx)
        }
        fn poll_sendfile(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            file_fd: i32,
            seek: u64,
            size: usize,
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut **self).poll_sendfile(cx, file_fd, seek, size)
        }

        fn is_write_msg(&self) -> bool {
            (**self).is_write_msg()
        }
        fn write_cache_size(&self) -> usize {
            (**self).write_cache_size()
        }
    };
}

impl<T: ?Sized + AsyncWriteMsg + Unpin> AsyncWriteMsg for Box<T> {
    deref_async_write_msg!();
}

impl<T: ?Sized + AsyncWriteMsg + Unpin> AsyncWriteMsg for &mut T {
    deref_async_write_msg!();
}

impl<P> AsyncWriteMsg for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncWriteMsg,
{
    fn poll_write_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_write_msg(cx, buf)
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_mut().as_mut().poll_is_write_msg(cx)
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().as_mut().poll_write_ready(cx)
    }
    fn poll_sendfile(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        self.get_mut()
            .as_mut()
            .poll_sendfile(cx, file_fd, seek, size)
    }

    fn is_write_msg(&self) -> bool {
        (**self).is_write_msg()
    }
    fn write_cache_size(&self) -> usize {
        (**self).write_cache_size()
    }
}

pub trait AsyncWriteMsgExt: AsyncWriteMsg {
    fn async_is_write_msg<'a>(&'a mut self) -> IsWriteMsg<'a, Self>
    where
        Self: Unpin,
    {
        is_write_msg(self)
    }

    fn write_msg<'a>(&'a mut self, buf: MsgWriteBuf) -> WriteMsg<'a, Self>
    where
        Self: Unpin,
    {
        write_msg(self, buf)
    }
    fn writable<'a>(&'a mut self) -> Writable<'a, Self>
    where
        Self: Unpin,
    {
        writable(self)
    }
    fn sendfile<'a>(&'a mut self, file_fd: i32, seek: u64, size: usize) -> Sendfile<'a, Self>
    where
        Self: Unpin,
    {
        sendfile(self, file_fd, seek, size)
    }
}

impl<W: AsyncWriteMsg + ?Sized> AsyncWriteMsgExt for W {}
