use crate::io::async_read_msg::AsyncReadMsg;
use crate::io::async_stream::AsyncStream;
use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
use crate::util::StreamReadMsg;
use std::cell::UnsafeCell;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

/// The readable half of a value returned from [`split`](split()).
pub struct ReadHalf<T> {
    inner: Arc<Inner<T>>,
}

/// The writable half of a value returned from [`split`](split()).
pub struct WriteHalf<T> {
    inner: Arc<Inner<T>>,
}

/// Splits a single value implementing `AsyncRead + AsyncWrite` into separate
/// `AsyncRead` and `AsyncWrite` handles.
///
/// To restore this read/write object from its `ReadHalf` and
/// `WriteHalf` use [`unsplit`](ReadHalf::unsplit()).
pub fn split<T>(stream: T) -> (ReadHalf<T>, WriteHalf<T>)
where
    T: AsyncRead + AsyncWrite + AsyncReadMsg + AsyncWriteMsg + AsyncStream,
{
    let inner = Arc::new(Inner {
        locked: AtomicBool::new(false),
        stream: UnsafeCell::new(stream),
    });

    let rd = ReadHalf {
        inner: inner.clone(),
    };

    let wr = WriteHalf { inner };

    (rd, wr)
}

struct Inner<T> {
    locked: AtomicBool,
    stream: UnsafeCell<T>,
}

struct Guard<'a, T> {
    inner: &'a Inner<T>,
}

impl<T> ReadHalf<T> {
    /// Checks if this `ReadHalf` and some `WriteHalf` were split from the same
    /// stream.
    pub fn is_pair_of(&self, other: &WriteHalf<T>) -> bool {
        other.is_pair_of(self)
    }

    /// Reunites with a previously split `WriteHalf`.
    ///
    /// # Panics
    ///
    /// If this `ReadHalf` and the given `WriteHalf` do not originate from the
    /// same `split` operation this method will panic.
    /// This can be checked ahead of time by comparing the stream ID
    /// of the two halves.
    #[track_caller]
    pub fn unsplit(self, wr: WriteHalf<T>) -> T {
        if self.is_pair_of(&wr) {
            drop(wr);

            let inner = Arc::try_unwrap(self.inner)
                .ok()
                .expect("`Arc::try_unwrap` failed");

            inner.stream.into_inner()
        } else {
            panic!("Unrelated `split::Write` passed to `split::Read::unsplit`.")
        }
    }
}

impl<T> WriteHalf<T> {
    /// Checks if this `WriteHalf` and some `ReadHalf` were split from the same
    /// stream.
    pub fn is_pair_of(&self, other: &ReadHalf<T>) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}
use crate::io::async_stream::Stream;
impl<T: Stream> crate::io::async_stream::Stream for ReadHalf<T> {
    fn raw_fd(&self) -> i32 {
        unsafe { &*self.inner.stream.get() }.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        unsafe { &*self.inner.stream.get() }.is_sendfile()
    }
}
impl<T: AsyncStream> crate::io::async_stream::AsyncStream for ReadHalf<T> {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_is_single(cx)
    }
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_raw_fd(cx)
    }
}

impl<T: Stream> crate::io::async_stream::Stream for WriteHalf<T> {
    fn raw_fd(&self) -> i32 {
        unsafe { &*self.inner.stream.get() }.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        unsafe { &*self.inner.stream.get() }.is_sendfile()
    }
}
impl<T: AsyncStream> crate::io::async_stream::AsyncStream for WriteHalf<T> {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_is_single(cx)
    }
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_raw_fd(cx)
    }
}

impl<T: AsyncReadMsg> crate::io::async_read_msg::AsyncReadMsg for ReadHalf<T> {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_try_read_msg(cx, msg_size)
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_read_msg(cx, msg_size)
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_is_read_msg(cx)
    }

    fn poll_try_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_try_read(cx, buf)
    }

    fn is_read_msg(&self) -> bool {
        unsafe { &*self.inner.stream.get() }.is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        unsafe { &*self.inner.stream.get() }.read_cache_size()
    }
}

impl<T: AsyncWriteMsg> crate::io::async_write_msg::AsyncWriteMsg for WriteHalf<T> {
    fn poll_write_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write_msg(cx, buf)
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_is_write_msg(cx)
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write_ready(cx)
    }
    fn poll_sendfile(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_sendfile(cx, file_fd, seek, size)
    }

    fn is_write_msg(&self) -> bool {
        unsafe { &*self.inner.stream.get() }.is_write_msg()
    }
    fn write_cache_size(&self) -> usize {
        unsafe { &*self.inner.stream.get() }.write_cache_size()
    }
}

impl<T: AsyncRead> AsyncRead for ReadHalf<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_read(cx, buf)
    }
}

impl<T: AsyncWrite> AsyncWrite for WriteHalf<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        unsafe { Pin::new_unchecked(&mut *self.inner.stream.get()) }.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_shutdown(cx)
    }
}

impl<T> Inner<T> {
    fn poll_lock(&self, cx: &mut Context<'_>) -> Poll<Guard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Acquire, Acquire)
            .is_ok()
        {
            Poll::Ready(Guard { inner: self })
        } else {
            // Spin... but investigate a better strategy

            std::thread::yield_now();
            cx.waker().wake_by_ref();

            Poll::Pending
        }
    }
}

impl<T> Guard<'_, T> {
    fn stream_pin(&mut self) -> Pin<&mut T> {
        // safety: the stream is pinned in `Arc` and the `Guard` ensures mutual
        // exclusion.
        unsafe { Pin::new_unchecked(&mut *self.inner.stream.get()) }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.inner.locked.store(false, Release);
    }
}

unsafe impl<T: Send> Send for ReadHalf<T> {}
unsafe impl<T: Send> Send for WriteHalf<T> {}
unsafe impl<T: Sync> Sync for ReadHalf<T> {}
unsafe impl<T: Sync> Sync for WriteHalf<T> {}

impl<T: fmt::Debug> fmt::Debug for ReadHalf<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("split::ReadHalf").finish()
    }
}

impl<T: fmt::Debug> fmt::Debug for WriteHalf<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("split::WriteHalf").finish()
    }
}
