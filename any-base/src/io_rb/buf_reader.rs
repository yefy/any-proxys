use crate::io::buf_reader::BufReader as IoBufReader;
use pin_project_lite::pin_project;
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, fmt};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

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
        pub(super) max_cap: usize,
        pub(super) reinit_cap: usize,
        pub(super) rollback: i64,
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
        let max_cap = buffer.len();
        Self {
            inner,
            buf: buffer.into_boxed_slice(),
            pos: 0,
            cap: 0,
            max_cap,
            reinit_cap: 0,
            rollback: -1,
        }
    }

    pub fn new_from_buffer(inner: R, buffer: (Box<[u8]>, usize, usize)) -> Self {
        let (buf, pos, cap) = buffer;
        let max_cap = buf.len();
        Self {
            inner,
            buf,
            pos,
            cap,
            max_cap,
            reinit_cap: 0,
            rollback: -1,
        }
    }

    pub fn new_from_buffer_ext(buffer: (R, Box<[u8]>, usize, usize)) -> Self {
        let (inner, buf, pos, cap) = buffer;
        let max_cap = buf.len();
        Self {
            inner,
            buf,
            pos,
            cap,
            max_cap,
            reinit_cap: 0,
            rollback: -1,
        }
    }

    pub fn table_buffer(self) -> (Box<[u8]>, usize, usize) {
        let BufReader {
            inner: _,
            buf,
            pos,
            cap,
            max_cap: _,
            reinit_cap: _,
            rollback: _,
        } = self;
        (buf, pos, cap)
    }

    pub fn table_buffer_ext(self) -> (R, Box<[u8]>, usize, usize) {
        let BufReader {
            inner,
            buf,
            pos,
            cap,
            max_cap: _,
            reinit_cap: _,
            rollback: _,
        } = self;
        (inner, buf, pos, cap)
    }

    pub fn to_io_buf_reader(self) -> IoBufReader<R> {
        IoBufReader::from_rb_buf_reader(self)
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

    pub fn set_reinit_cap(&mut self, reinit_cap: usize) {
        self.reinit_cap = reinit_cap;
    }

    pub fn set_reinit_cap_max(&mut self) {
        self.reinit_cap = self.buf.len();
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer(self: Pin<&mut Self>) {
        let me = self.project();
        *me.pos = 0;
        *me.cap = 0;
        *me.rollback = -1;
        if *me.reinit_cap > 0 && *me.reinit_cap <= me.buf.len() {
            *me.max_cap = *me.reinit_cap;
            *me.reinit_cap = 0;
        }
    }

    pub fn start(&mut self) {
        self.rollback = self.pos as i64;
    }

    pub fn rollback(&mut self) -> bool {
        if self.rollback >= 0 {
            self.pos = self.rollback as usize;
            self.commit();
            true
        } else {
            false
        }
    }

    pub fn commit(&mut self) -> bool {
        if self.rollback >= 0 {
            self.rollback = -1;
            true
        } else {
            false
        }
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
        if self.pos == self.cap && buf.remaining() >= self.max_cap {
            if self.rollback >= 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "*me.cap >= me.max_cap",
                )));
            }
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
            if *me.rollback >= 0 {
                if *me.cap >= *me.max_cap {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        "*me.cap >= me.max_cap",
                    )));
                }

                let mut buf = ReadBuf::new(&mut me.buf[*me.cap..*me.max_cap]);
                ready!(me.inner.poll_read(cx, &mut buf))?;
                *me.cap = *me.cap + buf.filled().len();
            } else {
                debug_assert!(*me.pos == *me.cap);
                if *me.reinit_cap > 0 && *me.reinit_cap <= me.buf.len() {
                    *me.max_cap = *me.reinit_cap;
                    *me.reinit_cap = 0;
                }
                let mut buf = ReadBuf::new(&mut me.buf[0..*me.max_cap]);
                ready!(me.inner.poll_read(cx, &mut buf))?;
                *me.cap = buf.filled().len();
                *me.pos = 0;
                *me.rollback = -1;
            }
        }

        Poll::Ready(Ok(&me.buf[*me.pos..*me.cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        *me.pos = cmp::min(*me.pos + amt, *me.cap);
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
        self.get_ref().is_write_vectored()
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
                &format_args!("{}/{}", self.cap - self.pos, self.max_cap),
            )
            .finish()
    }
}
