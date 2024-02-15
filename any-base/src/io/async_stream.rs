use crate::io::is_single::is_single;
use crate::io::is_single::IsSingle;
use crate::io::raw_fd::{async_raw_fd, AsyncRawFd};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait Stream {
    fn raw_fd(&self) -> i32;
    fn is_sendfile(&self) -> bool;
}

pub trait AsyncStream {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool>;
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32>;
}

macro_rules! deref_async_stream {
    () => {
        fn poll_is_single(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
            Pin::new(&mut **self).poll_is_single(cx)
        }
        fn poll_raw_fd(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
            Pin::new(&mut **self).poll_raw_fd(cx)
        }
    };
}

impl<T: ?Sized + AsyncStream + Unpin> AsyncStream for Box<T> {
    deref_async_stream!();
}

impl<T: ?Sized + AsyncStream + Unpin> AsyncStream for &mut T {
    deref_async_stream!();
}

impl<P> AsyncStream for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncStream,
{
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        self.get_mut().as_mut().poll_is_single(cx)
    }
    fn poll_raw_fd(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        self.get_mut().as_mut().poll_raw_fd(cx)
    }
}

pub trait AsyncStreamExt: AsyncStream {
    fn is_single<'a>(&'a mut self) -> IsSingle<'a, Self>
    where
        Self: Unpin,
    {
        is_single(self)
    }
    fn async_raw_fd<'a>(&'a mut self) -> AsyncRawFd<'a, Self>
    where
        Self: Unpin,
    {
        async_raw_fd(self)
    }
}

impl<R: AsyncStream + ?Sized> AsyncStreamExt for R {}
