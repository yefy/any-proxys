use pin_project_lite::pin_project;
use std::fmt;
use std::io::{self, IoSlice, SeekFrom, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

use crate::ready;
use crate::DEFAULT_BUF_SIZE;

pin_project! {
    /// Wraps a writer and buffers its output.
    ///
    /// It can be excessively inefficient to work directly with something that
    /// implements [`AsyncWrite`]. A `BufWriter` keeps an in-memory buffer of data and
    /// writes it to an underlying writer in large, infrequent batches.
    ///
    /// `BufWriter` can improve the speed of programs that make *small* and
    /// *repeated* write calls to the same file or network socket. It does not
    /// help when writing very large amounts at once, or writing just one or a few
    /// times. It also provides no advantage when writing to a destination that is
    /// in memory, like a `Vec<u8>`.
    ///
    /// When the `BufWriter` is dropped, the contents of its buffer will be
    /// discarded. Creating multiple instances of a `BufWriter` on the same
    /// stream can cause data loss. If you need to write out the contents of its
    /// buffer, you must manually call flush before the writer is dropped.
    ///
    /// [`AsyncWrite`]: AsyncWrite
    /// [`flush`]: super::AsyncWriteExt::flush
    ///
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct BufWriter<W> {
        #[pin]
        pub(super) inner: W,
        pub(super) buf: Vec<u8>,
        pub(super) written: usize,
        pub(super) seek_state: SeekState,
    }
}

impl<W: AsyncWrite> BufWriter<W> {
    /// Creates a new `BufWriter` with a default buffer capacity. The default is currently 8 KB,
    /// but may change in the future.
    pub fn new(inner: W) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufWriter` with the specified buffer capacity.
    pub fn with_capacity(cap: usize, inner: W) -> Self {
        Self {
            inner,
            buf: Vec::with_capacity(cap),
            written: 0,
            seek_state: SeekState::Init,
        }
    }

    fn flush_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut me = self.project();

        let len = me.buf.len();
        let mut ret = Ok(());
        while *me.written < len {
            match ready!(me.inner.as_mut().poll_write(cx, &me.buf[*me.written..])) {
                Ok(0) => {
                    ret = Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write the buffered data",
                    ));
                    break;
                }
                Ok(n) => *me.written += n,
                Err(e) => {
                    ret = Err(e);
                    break;
                }
            }
        }
        if *me.written > 0 {
            me.buf.drain(..*me.written);
        }
        *me.written = 0;
        Poll::Ready(ret)
    }

    /// Gets a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Gets a mutable reference to the underlying writer.
    ///
    /// It is inadvisable to directly write to the underlying writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying writer.
    ///
    /// It is inadvisable to directly write to the underlying writer.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut W> {
        self.project().inner
    }

    /// Consumes this `BufWriter`, returning the underlying writer.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Returns a reference to the internally buffered data.
    pub fn buffer(&self) -> &[u8] {
        &self.buf
    }
}
use crate::io::async_stream::Stream;
impl<W: AsyncWrite + Stream> Stream for BufWriter<W> {
    fn raw_fd(&self) -> i32 {
        self.inner.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        self.inner.is_sendfile()
    }
}
use crate::io::async_stream::AsyncStream;
impl<W: AsyncWrite + AsyncStream> AsyncStream for BufWriter<W> {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_single(cx)
    }
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        self.get_pin_mut().poll_raw_fd(cx)
    }
}

use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
impl<W: AsyncWrite + AsyncWriteMsg> AsyncWriteMsg for BufWriter<W> {
    fn poll_write_msg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        ready!(self.as_mut().flush_buf(cx))?;
        self.get_pin_mut().poll_write_msg(cx, buf)
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_write_msg(cx)
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_write_ready(cx)
    }

    fn poll_sendfile(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        ready!(self.as_mut().flush_buf(cx))?;
        self.get_pin_mut().poll_sendfile(cx, file_fd, seek, size)
    }

    fn is_write_msg(&self) -> bool {
        self.inner.is_write_msg()
    }
    fn write_cache_size(&self) -> usize {
        self.buf.len() + self.inner.write_cache_size()
    }
}

impl<W: AsyncWrite> AsyncWrite for BufWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.buf.len() + buf.len() > self.buf.capacity() {
            ready!(self.as_mut().flush_buf(cx))?;
        }

        let me = self.project();
        if buf.len() >= me.buf.capacity() {
            me.inner.poll_write(cx, buf)
        } else {
            Poll::Ready(me.buf.write(buf))
        }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        if self.inner.is_write_vectored() {
            let total_len = bufs
                .iter()
                .fold(0usize, |acc, b| acc.saturating_add(b.len()));
            if total_len > self.buf.capacity() - self.buf.len() {
                ready!(self.as_mut().flush_buf(cx))?;
            }
            let me = self.as_mut().project();
            if total_len >= me.buf.capacity() {
                // It's more efficient to pass the slices directly to the
                // underlying writer than to buffer them.
                // The case when the total_len calculation saturates at
                // usize::MAX is also handled here.
                me.inner.poll_write_vectored(cx, bufs)
            } else {
                bufs.iter().for_each(|b| me.buf.extend_from_slice(b));
                Poll::Ready(Ok(total_len))
            }
        } else {
            // Remove empty buffers at the beginning of bufs.
            while bufs.first().map(|buf| buf.len()) == Some(0) {
                bufs = &bufs[1..];
            }
            if bufs.is_empty() {
                return Poll::Ready(Ok(0));
            }
            // Flush if the first buffer doesn't fit.
            let first_len = bufs[0].len();
            if first_len > self.buf.capacity() - self.buf.len() {
                ready!(self.as_mut().flush_buf(cx))?;
                debug_assert!(self.buf.is_empty());
            }
            let me = self.as_mut().project();
            if first_len >= me.buf.capacity() {
                // The slice is at least as large as the buffering capacity,
                // so it's better to write it directly, bypassing the buffer.
                debug_assert!(me.buf.is_empty());
                return me.inner.poll_write(cx, &bufs[0]);
            } else {
                me.buf.extend_from_slice(&bufs[0]);
                bufs = &bufs[1..];
            }
            let mut total_written = first_len;
            debug_assert!(total_written != 0);
            // Append the buffers that fit in the internal buffer.
            for buf in bufs {
                if buf.len() > me.buf.capacity() - me.buf.len() {
                    break;
                } else {
                    me.buf.extend_from_slice(buf);
                    total_written += buf.len();
                }
            }
            Poll::Ready(Ok(total_written))
        }
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().flush_buf(cx))?;
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().flush_buf(cx))?;
        self.get_pin_mut().poll_shutdown(cx)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SeekState {
    /// start_seek has not been called.
    Init,
    /// start_seek has been called, but poll_complete has not yet been called.
    Start(SeekFrom),
    /// Waiting for completion of poll_complete.
    Pending,
}

/// Seek to the offset, in bytes, in the underlying writer.
///
/// Seeking always writes out the internal buffer before seeking.
impl<W: AsyncWrite + AsyncSeek> AsyncSeek for BufWriter<W> {
    fn start_seek(self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        // We need to flush the internal buffer before seeking.
        // It receives a `Context` and returns a `Poll`, so it cannot be called
        // inside `start_seek`.
        *self.project().seek_state = SeekState::Start(pos);
        Ok(())
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let pos = match self.seek_state {
            SeekState::Init => {
                return self.project().inner.poll_complete(cx);
            }
            SeekState::Start(pos) => Some(pos),
            SeekState::Pending => None,
        };

        // Flush the internal buffer before seeking.
        ready!(self.as_mut().flush_buf(cx))?;

        let mut me = self.project();
        if let Some(pos) = pos {
            // Ensure previous seeks have finished before starting a new one
            ready!(me.inner.as_mut().poll_complete(cx))?;
            if let Err(e) = me.inner.as_mut().start_seek(pos) {
                *me.seek_state = SeekState::Init;
                return Poll::Ready(Err(e));
            }
        }
        match me.inner.poll_complete(cx) {
            Poll::Ready(res) => {
                *me.seek_state = SeekState::Init;
                Poll::Ready(res)
            }
            Poll::Pending => {
                *me.seek_state = SeekState::Pending;
                Poll::Pending
            }
        }
    }
}

use crate::io::async_read_msg::AsyncReadMsg;
use crate::util::StreamReadMsg;

impl<W: AsyncWrite + AsyncReadMsg> AsyncReadMsg for BufWriter<W> {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        self.get_pin_mut().poll_try_read_msg(cx, msg_size)
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        self.get_pin_mut().poll_read_msg(cx, msg_size)
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_read_msg(cx)
    }

    fn poll_try_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_try_read(cx, buf)
    }
    fn is_read_msg(&self) -> bool {
        self.inner.is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        self.inner.read_cache_size()
    }
}

impl<W: AsyncWrite + AsyncRead> AsyncRead for BufWriter<W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_read(cx, buf)
    }
}

impl<W: AsyncWrite + AsyncBufRead> AsyncBufRead for BufWriter<W> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        self.get_pin_mut().poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.get_pin_mut().consume(amt);
    }
}

impl<W: fmt::Debug> fmt::Debug for BufWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufWriter")
            .field("writer", &self.inner)
            .field(
                "buffer",
                &format_args!("{}/{}", self.buf.len(), self.buf.capacity()),
            )
            .field("written", &self.written)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<BufWriter<()>>();
    }
}
