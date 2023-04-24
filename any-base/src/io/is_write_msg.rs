use crate::io::async_write_msg::AsyncWriteMsg;
use pin_project_lite::pin_project;
use std::future::Future;
use std::marker::PhantomPinned;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub(crate) fn is_write_msg<'a, R>(reader: &'a mut R) -> IsWriteMsg<'a, R>
where
    R: AsyncWriteMsg + Unpin + ?Sized,
{
    IsWriteMsg {
        reader,
        _pin: PhantomPinned,
    }
}

pin_project! {
    /// A future which can be used to easily read available number of bytes to fill
    /// a buffer.
    ///
    /// Created by the [`read`] function.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct IsWriteMsg<'a, R: ?Sized> {
        reader: &'a mut R,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<R> Future for IsWriteMsg<'_, R>
where
    R: AsyncWriteMsg + Unpin + ?Sized,
{
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let me = self.project();
        Pin::new(me.reader).poll_is_write_msg(cx)
    }
}
