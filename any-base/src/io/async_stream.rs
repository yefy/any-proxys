use crate::io::is_single::{is_single, IsSingle};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait AsyncStream {
    fn poll_is_single(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool>;
}

macro_rules! deref_async_stream {
    () => {
        fn poll_is_single(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
            Pin::new(&mut **self).poll_is_single(cx)
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
}

pub trait AsyncStreamExt: AsyncStream {
    fn is_single<'a>(&'a mut self) -> IsSingle<'a, Self>
    where
        Self: Unpin,
    {
        is_single(self)
    }
}

impl<R: AsyncStream + ?Sized> AsyncStreamExt for R {}
