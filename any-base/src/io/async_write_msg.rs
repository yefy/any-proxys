use crate::io::is_write_msg::{is_write_msg, IsWriteMsg};
use crate::io::write_msg::{write_msg, WriteMsg};
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

pub struct AsyncWriteBuf {
    buf: Option<Vec<u8>>,
    len: usize,
}

impl AsyncWriteBuf {
    pub fn new(buf: Vec<u8>) -> AsyncWriteBuf {
        let len = buf.len();
        AsyncWriteBuf {
            buf: Some(buf),
            len,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn data(&mut self) -> Vec<u8> {
        self.buf.take().unwrap()
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
    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool>;
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
}

pub trait AsyncWriteMsgExt: AsyncWriteMsg {
    fn is_write_msg<'a>(&'a mut self) -> IsWriteMsg<'a, Self>
    where
        Self: Unpin,
    {
        is_write_msg(self)
    }

    fn write_msg<'a>(&'a mut self, buf: Vec<u8>) -> WriteMsg<'a, Self>
    where
        Self: Unpin,
    {
        write_msg(self, buf)
    }
}

impl<W: AsyncWriteMsg + ?Sized> AsyncWriteMsgExt for W {}
