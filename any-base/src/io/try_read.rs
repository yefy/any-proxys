use crate::io::async_read_msg::AsyncReadMsg;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub(crate) fn try_read<'a, R>(reader: &'a mut R, buf: &'a mut [u8]) -> TryRead<'a, R>
where
    R: AsyncReadMsg + Unpin + ?Sized,
{
    TryRead {
        reader,
        buf,
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
    pub struct TryRead<'a, R: ?Sized> {
        reader: &'a mut R,
        buf: &'a mut [u8],
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<R> Future for TryRead<'_, R>
where
    R: AsyncReadMsg + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = self.project();
        let mut buf = ReadBuf::new(me.buf);
        ready!(Pin::new(me.reader).poll_try_read(cx, &mut buf))?;
        Poll::Ready(Ok(buf.filled().len()))
    }
}
