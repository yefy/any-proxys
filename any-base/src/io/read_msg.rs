use crate::io::async_read_msg::AsyncReadMsg;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub(crate) fn read_msg<'a, R>(reader: &'a mut R) -> ReadMsg<'a, R>
where
    R: AsyncReadMsg + Unpin + ?Sized,
{
    ReadMsg {
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
    pub struct ReadMsg<'a, R: ?Sized> {
        reader: &'a mut R,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<R> Future for ReadMsg<'_, R>
where
    R: AsyncReadMsg + Unpin + ?Sized,
{
    type Output = io::Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<Vec<u8>>> {
        let me = self.project();
        Pin::new(me.reader).poll_read_msg(cx)
    }
}