use crate::io_rb::buf_reader::BufReader as RbBufReader;
use pin_project_lite::pin_project;
use std::io::{self, IoSlice, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, fmt, mem};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

use crate::ready;
use crate::DEFAULT_BUF_SIZE;

pin_project! {
    /// The `BufReader` struct adds buffering to any reader.
    ///
    /// It can be excessively inefficient to work directly with a [`AsyncRead`]
    /// instance. A `BufReader` performs large, infrequent reads on the underlying
    /// [`AsyncRead`] and maintains an in-memory buffer of the results.
    ///
    /// `BufReader` can improve the speed of programs that make *small* and
    /// *repeated* read calls to the same file or network socket. It does not
    /// help when reading very large amounts at once, or reading just one or a few
    /// times. It also provides no advantage when reading from a source that is
    /// already in memory, like a `Vec<u8>`.
    ///
    /// When the `BufReader` is dropped, the contents of its buffer will be
    /// discarded. Creating multiple instances of a `BufReader` on the same
    /// stream can cause data loss.
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct BufReader<R> {
        #[pin]
        pub(super) inner: R,
        pub(super) buf: Box<[u8]>,
        pub(super) pos: usize,
        pub(super) cap: usize,
        pub(super) seek_state: SeekState,
    }
}

impl<R: AsyncRead> BufReader<R> {
    /// Creates a new `BufReader` with a default buffer capacity. The default is currently 8 KB,
    /// but may change in the future.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader` with the specified buffer capacity.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        let buffer = vec![0; capacity];
        Self {
            inner,
            buf: buffer.into_boxed_slice(),
            pos: 0,
            cap: 0,
            seek_state: SeekState::Init,
        }
    }

    /// Gets a reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Consumes this `BufReader`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Returns a reference to the internally buffered data.
    ///
    /// Unlike `fill_buf`, this will not attempt to fill the buffer if it is empty.
    pub fn buffer(&self) -> &[u8] {
        &self.buf[self.pos..self.cap]
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer(self: Pin<&mut Self>) {
        let me = self.project();
        *me.pos = 0;
        *me.cap = 0;
    }

    pub fn from_rb_buf_reader(buf_reader: RbBufReader<R>) -> Self {
        let (inner, buf, pos, cap) = buf_reader.table_buffer_ext();
        Self {
            inner,
            buf,
            pos,
            cap,
            seek_state: SeekState::Init,
        }
    }

    pub fn table_buffer_ext(self) -> (R, Box<[u8]>, usize, usize) {
        let BufReader {
            inner,
            buf,
            pos,
            cap,
            seek_state: _,
        } = self;
        (inner, buf, pos, cap)
    }
}

use crate::io::async_stream::Stream;
impl<R: AsyncRead + Stream> Stream for BufReader<R> {
    fn raw_fd(&self) -> i32 {
        self.inner.raw_fd()
    }
    fn is_sendfile(&self) -> bool {
        self.inner.is_sendfile()
    }
}

use crate::io::async_stream::AsyncStream;
impl<R: AsyncRead + AsyncStream> AsyncStream for BufReader<R> {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_single(cx)
    }
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        self.get_pin_mut().poll_raw_fd(cx)
    }
}

use crate::io::async_read_msg::AsyncReadMsg;
impl<R: AsyncRead + AsyncReadMsg> AsyncReadMsg for BufReader<R> {
    fn poll_try_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let len = self.buffer().len();
        if len > 0 {
            let mut stream_msg = StreamReadMsg::new();
            stream_msg.push_back_data(Bytes::from(self.buffer().to_vec()));
            self.consume(len);
            return Poll::Ready(Ok(stream_msg));
        }
        self.get_pin_mut().poll_try_read_msg(cx, msg_size)
    }
    fn poll_read_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        msg_size: usize,
    ) -> Poll<io::Result<StreamReadMsg>> {
        let len = self.buffer().len();
        if len > 0 {
            let mut stream_msg = StreamReadMsg::new();
            stream_msg.push_back_data(Bytes::from(self.buffer().to_vec()));
            self.consume(len);
            return Poll::Ready(Ok(stream_msg));
        }
        self.get_pin_mut().poll_read_msg(cx, msg_size)
    }

    fn poll_is_read_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_read_msg(cx)
    }

    fn poll_try_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.remaining() >= self.buf.len() {
            let res = ready!(self.as_mut().get_pin_mut().poll_try_read(cx, buf));
            self.discard_buffer();
            return Poll::Ready(res);
        }

        let rem = {
            let me = self.as_mut().project();

            // If we've reached the end of our internal buffer then we need to fetch
            // some more data from the underlying reader.
            // Branch using `>=` instead of the more correct `==`
            // to tell the compiler that the pos..cap slice is always valid.
            if *me.pos >= *me.cap {
                debug_assert!(*me.pos == *me.cap);
                let mut buf = ReadBuf::new(me.buf);
                ready!(me.inner.poll_try_read(cx, &mut buf))?;
                *me.cap = buf.filled().len();
                *me.pos = 0;
            }
            &me.buf[*me.pos..*me.cap]
        };

        let amt = std::cmp::min(rem.len(), buf.remaining());
        if amt > 0 {
            buf.put_slice(&rem[..amt]);
            self.consume(amt);
        }
        Poll::Ready(Ok(()))
    }

    fn is_read_msg(&self) -> bool {
        self.inner.is_read_msg()
    }
    fn read_cache_size(&self) -> usize {
        self.buffer().len() + self.inner.read_cache_size()
    }
}

impl<R: AsyncRead> AsyncRead for BufReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.remaining() >= self.buf.len() {
            let res = ready!(self.as_mut().get_pin_mut().poll_read(cx, buf));
            self.discard_buffer();
            return Poll::Ready(res);
        }
        let rem = ready!(self.as_mut().poll_fill_buf(cx))?;
        let amt = std::cmp::min(rem.len(), buf.remaining());
        buf.put_slice(&rem[..amt]);
        self.consume(amt);
        Poll::Ready(Ok(()))
    }
}

impl<R: AsyncRead> AsyncBufRead for BufReader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.project();

        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the underlying reader.
        // Branch using `>=` instead of the more correct `==`
        // to tell the compiler that the pos..cap slice is always valid.
        if *me.pos >= *me.cap {
            debug_assert!(*me.pos == *me.cap);
            let mut buf = ReadBuf::new(me.buf);
            ready!(me.inner.poll_read(cx, &mut buf))?;
            *me.cap = buf.filled().len();
            *me.pos = 0;
        }
        Poll::Ready(Ok(&me.buf[*me.pos..*me.cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        *me.pos = cmp::min(*me.pos + amt, *me.cap);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SeekState {
    /// start_seek has not been called.
    Init,
    /// start_seek has been called, but poll_complete has not yet been called.
    Start(SeekFrom),
    /// Waiting for completion of the first poll_complete in the `n.checked_sub(remainder).is_none()` branch.
    PendingOverflowed(i64),
    /// Waiting for completion of poll_complete.
    Pending,
}

/// Seeks to an offset, in bytes, in the underlying reader.
///
/// The position used for seeking with `SeekFrom::Current(_)` is the
/// position the underlying reader would be at if the `BufReader` had no
/// internal buffer.
///
/// Seeking always discards the internal buffer, even if the seek position
/// would otherwise fall within it. This guarantees that calling
/// `.into_inner()` immediately after a seek yields the underlying reader
/// at the same position.
///
/// See [`AsyncSeek`] for more details.
///
/// Note: In the edge case where you're seeking with `SeekFrom::Current(n)`
/// where `n` minus the internal buffer length overflows an `i64`, two
/// seeks will be performed instead of one. If the second seek returns
/// `Err`, the underlying reader will be left at the same position it would
/// have if you called `seek` with `SeekFrom::Current(0)`.
impl<R: AsyncRead + AsyncSeek> AsyncSeek for BufReader<R> {
    fn start_seek(self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        // We needs to call seek operation multiple times.
        // And we should always call both start_seek and poll_complete,
        // as start_seek alone cannot guarantee that the operation will be completed.
        // poll_complete receives a Context and returns a Poll, so it cannot be called
        // inside start_seek.
        *self.project().seek_state = SeekState::Start(pos);
        Ok(())
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let res = match mem::replace(self.as_mut().project().seek_state, SeekState::Init) {
            SeekState::Init => {
                // 1.x AsyncSeek recommends calling poll_complete before start_seek.
                // We don't have to guarantee that the value returned by
                // poll_complete called without start_seek is correct,
                // so we'll return 0.
                return Poll::Ready(Ok(0));
            }
            SeekState::Start(SeekFrom::Current(n)) => {
                let remainder = (self.cap - self.pos) as i64;
                // it should be safe to assume that remainder fits within an i64 as the alternative
                // means we managed to allocate 8 exbibytes and that's absurd.
                // But it's not out of the realm of possibility for some weird underlying reader to
                // support seeking by i64::MIN so we need to handle underflow when subtracting
                // remainder.
                if let Some(offset) = n.checked_sub(remainder) {
                    self.as_mut()
                        .get_pin_mut()
                        .start_seek(SeekFrom::Current(offset))?;
                } else {
                    // seek backwards by our remainder, and then by the offset
                    self.as_mut()
                        .get_pin_mut()
                        .start_seek(SeekFrom::Current(-remainder))?;
                    if self.as_mut().get_pin_mut().poll_complete(cx)?.is_pending() {
                        *self.as_mut().project().seek_state = SeekState::PendingOverflowed(n);
                        return Poll::Pending;
                    }

                    // https://github.com/rust-lang/rust/pull/61157#issuecomment-495932676
                    self.as_mut().discard_buffer();

                    self.as_mut()
                        .get_pin_mut()
                        .start_seek(SeekFrom::Current(n))?;
                }
                self.as_mut().get_pin_mut().poll_complete(cx)?
            }
            SeekState::PendingOverflowed(n) => {
                if self.as_mut().get_pin_mut().poll_complete(cx)?.is_pending() {
                    *self.as_mut().project().seek_state = SeekState::PendingOverflowed(n);
                    return Poll::Pending;
                }

                // https://github.com/rust-lang/rust/pull/61157#issuecomment-495932676
                self.as_mut().discard_buffer();

                self.as_mut()
                    .get_pin_mut()
                    .start_seek(SeekFrom::Current(n))?;
                self.as_mut().get_pin_mut().poll_complete(cx)?
            }
            SeekState::Start(pos) => {
                // Seeking with Start/End doesn't care about our buffer length.
                self.as_mut().get_pin_mut().start_seek(pos)?;
                self.as_mut().get_pin_mut().poll_complete(cx)?
            }
            SeekState::Pending => self.as_mut().get_pin_mut().poll_complete(cx)?,
        };

        match res {
            Poll::Ready(res) => {
                self.discard_buffer();
                Poll::Ready(Ok(res))
            }
            Poll::Pending => {
                *self.as_mut().project().seek_state = SeekState::Pending;
                Poll::Pending
            }
        }
    }
}

use crate::io::async_write_msg::{AsyncWriteBuf, AsyncWriteMsg};
use crate::util::StreamReadMsg;
use bytes::Bytes;

impl<R: AsyncRead + AsyncWriteMsg> AsyncWriteMsg for BufReader<R> {
    fn poll_write_msg(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut AsyncWriteBuf,
    ) -> Poll<io::Result<usize>> {
        self.get_pin_mut().poll_write_msg(cx, buf)
    }

    fn poll_is_write_msg(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_pin_mut().poll_is_write_msg(cx)
    }
    fn poll_write_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_write_ready(cx)
    }

    fn poll_sendfile(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        file_fd: i32,
        seek: u64,
        size: usize,
    ) -> Poll<io::Result<usize>> {
        self.get_pin_mut().poll_sendfile(cx, file_fd, seek, size)
    }

    fn is_write_msg(&self) -> bool {
        self.inner.is_write_msg()
    }

    fn write_cache_size(&self) -> usize {
        self.inner.write_cache_size()
    }
}

impl<R: AsyncRead + AsyncWrite> AsyncWrite for BufReader<R> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_pin_mut().poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.get_pin_mut().poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_shutdown(cx)
    }
}

impl<R: fmt::Debug> fmt::Debug for BufReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufReader")
            .field("reader", &self.inner)
            .field(
                "buffer",
                &format_args!("{}/{}", self.cap - self.pos, self.buf.len()),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<BufReader<()>>();
    }
}
