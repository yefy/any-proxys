#[allow(unused_imports)]
//
use {
    crate::BlockingHandle,
    blanket::blanket,
    futures_task::{FutureObj, SpawnError},
    futures_util::{
        future::{abortable, FutureExt},
        task::SpawnExt,
    },
    std::{
        future::Future,
        pin::Pin,
        rc::Rc,
        sync::{atomic::AtomicBool, Arc},
    },
};

/// Indicate the executor can provide a threadpool for blocking operations.
/// There is two methods of this trait. One of them requires boxing the closure
/// and the other is not object safe.
//
// Doesn't work with blanket.
// #[ blanket(derive( Mut, Box, Arc, Rc )) ]
//
pub trait SpawnBlocking<R>
where
    R: Send + 'static,
{
    /// Runs the provided closure on a thread where blocking is acceptable.
    //
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        Self: Sized;

    /// Runs the provided closure on a thread where blocking is acceptable. This part of the trait is
    /// object safe but your closure must be boxed and you cannot have a return value.
    //
    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R>;
}

impl<R, T> SpawnBlocking<R> for &T
where
    T: SpawnBlocking<R>,
    R: Send + 'static,
{
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        T: Sized,
    {
        (**self).spawn_blocking(f)
    }

    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R> {
        (**self).spawn_blocking_dyn(f)
    }
}

impl<R, T: SpawnBlocking<R>> SpawnBlocking<R> for &mut T
where
    T: SpawnBlocking<R>,
    R: Send + 'static,
{
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        T: Sized,
    {
        (**self).spawn_blocking(f)
    }

    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R> {
        (**self).spawn_blocking_dyn(f)
    }
}

impl<R, T: SpawnBlocking<R>> SpawnBlocking<R> for Box<T>
where
    T: SpawnBlocking<R>,
    R: Send + 'static,
{
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        T: Sized,
    {
        (**self).spawn_blocking(f)
    }

    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R> {
        (**self).spawn_blocking_dyn(f)
    }
}

impl<R, T: SpawnBlocking<R>> SpawnBlocking<R> for Arc<T>
where
    T: SpawnBlocking<R>,
    R: Send + 'static,
{
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        T: Sized,
    {
        (**self).spawn_blocking(f)
    }

    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R> {
        (**self).spawn_blocking_dyn(f)
    }
}

impl<R, T: SpawnBlocking<R>> SpawnBlocking<R> for Rc<T>
where
    T: SpawnBlocking<R>,
    R: Send + 'static,
{
    fn spawn_blocking<F>(&self, f: F) -> BlockingHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        T: Sized,
    {
        (**self).spawn_blocking(f)
    }

    fn spawn_blocking_dyn(&self, f: Box<dyn FnOnce() -> R + Send>) -> BlockingHandle<R> {
        (**self).spawn_blocking_dyn(f)
    }
}
