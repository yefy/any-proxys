use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
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
pub(crate) fn write_msg<'a, R>(reader: &'a mut R, buf: Vec<u8>) -> WriteMsg<'a, R>
where
    R: AsyncWriteMsg + Unpin + ?Sized,
{
    WriteMsg {
        reader,
        buf: AsyncWriteBuf::new(buf),
        _pin: PhantomPinned,
    }
}

pin_project! {
    /// A future which can be used to easily read available number of bytes to fill
    /// a buffer.
    ///
    /// Created by the [`read`] function.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct WriteMsg<'a, R: ?Sized> {
        reader: &'a mut R,
        buf: AsyncWriteBuf,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<R> Future for WriteMsg<'_, R>
where
    R: AsyncWriteMsg + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let mut me = self.project();
        Pin::new(me.reader).poll_write_msg(cx, &mut me.buf)
    }
}
