use crate::io::is_read_msg::is_read_msg;
use crate::io::is_read_msg::IsReadMsg;
use crate::io::read_msg::read_msg;
use crate::io::read_msg::ReadMsg;
use crate::io::try_read::{try_read, TryRead};
use crate::io::try_read_msg::try_read_msg;
use crate::io::try_read_msg::TryReadMsg;
use crate::util::StreamReadMsg;
use std::io;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

/// Reads bytes from a source.
///
/// This trait is analogous to the [`std::io::Read`] trait, but integrates with
/// the asynchronous task system. In particular, the [`poll_read`] method,
/// unlike [`Read::read`], will automatically queue the current task for wakeup
/// and return if data is not yet available, rather than blocking the calling
/// thread.
///
/// Specifically, this means that the `poll_read` function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(()))` means that data was immediately read and placed into
///   the output buffer. The amount of data read can be determined by the
///   increase in the length of the slice returned by `ReadBuf::filled`. If the
///   difference is 0, EOF has been reached.
///
/// * `Poll::Pending` means that no data was read into the buffer
///   provided. The I/O object is not currently readable but may become readable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is readable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   readable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `read` method only works in the
/// context of a future's task. The object may panic if used outside of a task.
///
/// Utilities for working with `AsyncRead` values are provided by
/// [`AsyncReadExt`].
///
/// [`poll_read`]: AsyncRead::poll_read
/// [`std::io::Read`]: std::io::Read
/// [`Read::read`]: std::io::Read::read
/// [`AsyncReadExt`]: crate::io::AsyncReadExt
pub trait AsyncReadMsg {
    /// Attempts to read from the `AsyncRead` into `buf`.
    ///
    /// On success, returns `Poll::Ready(Ok(()))` and places data in the
    /// unfilled portion of `buf`. If no data was read (`buf.filled().len()` is
    /// unchanged), it implies that EOF has been reached.
    ///
    /// If no data is available for reading, the method returns `Poll::Pending`
    /// and arranges for the current task (via `cx.waker()`) to receive a
    /// notification when the object becomes readable or is closed.
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>>;
    fn poll_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>>;
    fn is_read_msg(&self) -> bool;
    fn read_cache_size(&self) -> usize;
    fn poll_is_read_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool>;

    fn poll_try_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>>;
}

macro_rules! deref_async_read_msg {
    () => {
        fn poll_try_read_msg(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            msg_size: usize,
        ) -> Poll<io::Result<StreamReadMsg>> {
            Pin::new(&mut **self).poll_try_read_msg(cx, msg_size)
        }
        fn poll_read_msg(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            msg_size: usize,
        ) -> Poll<io::Result<StreamReadMsg>> {
            Pin::new(&mut **self).poll_read_msg(cx, msg_size)
        }
        fn poll_is_read_msg(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
            Pin::new(&mut **self).poll_is_read_msg(cx)
        }
        fn poll_try_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut **self).poll_try_read(cx, buf)
        }

        fn is_read_msg(&self) -> bool {
            (**self).is_read_msg()
        }
        fn read_cache_size(&self) -> usize {
            (**self).read_cache_size()
        }
    };
}

impl<T: ?Sized + AsyncReadMsg + Unpin> AsyncReadMsg for Box<T> {
    deref_async_read_msg!();
}

impl<T: ?Sized + AsyncReadMsg + Unpin> AsyncReadMsg for &mut T {
    deref_async_read_msg!();
}

impl<P> AsyncReadMsg for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncReadMsg,
{
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        self.get_mut().as_mut().poll_try_read_msg(cx, msg_size)
    }

    fn poll_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        self.get_mut().as_mut().poll_read_msg(cx, msg_size)
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_mut().as_mut().poll_is_read_msg(cx)
    }
    fn poll_try_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.get_mut().as_mut().poll_try_read(cx, buf)
    }

    fn is_read_msg(&self) -> bool {
        (**self).is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        (**self).read_cache_size()
    }
}

pub trait AsyncReadMsgExt: AsyncReadMsg {
    fn async_is_read_msg<'a>(&'a mut self) -> IsReadMsg<'a, Self>
    where
        Self: Unpin,
    {
        is_read_msg(self)
    }

    fn read_msg<'a>(&'a mut self, msg_size: usize) -> ReadMsg<'a, Self>
    where
        Self: Unpin,
    {
        read_msg(self, msg_size)
    }
    fn try_read_msg<'a>(&'a mut self, msg_size: usize) -> TryReadMsg<'a, Self>
    where
        Self: Unpin,
    {
        try_read_msg(self, msg_size)
    }
    fn try_read<'a>(&'a mut self, buf: &'a mut [u8]) -> TryRead<'a, Self>
    where
        Self: Unpin,
    {
        try_read(self, buf)
    }
}

impl<R: AsyncReadMsg + ?Sized> AsyncReadMsgExt for R {}
