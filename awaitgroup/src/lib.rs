//! [![Documentation](https://img.shields.io/badge/docs-0.6.0-4d76ae?style=for-the-badge)](https://docs.rs/awaitgroup/0.6.0)
//! [![Version](https://img.shields.io/crates/v/awaitgroup?style=for-the-badge)](https://crates.io/crates/awaitgroup)
//! [![License](https://img.shields.io/crates/l/awaitgroup?style=for-the-badge)](https://crates.io/crates/awaitgroup)
//! [![Actions](https://img.shields.io/github/workflow/status/ibraheemdev/awaitgroup/Rust/master?style=for-the-badge)](https://github.com/ibraheemdev/awaitgroup/actions)
//!
//! An asynchronous implementation of a `WaitGroup`.
//!
//! A `WaitGroup` waits for a collection of tasks to finish. The main task can create new workers and
//! pass them to each of the tasks it wants to wait for. Then, each of the tasks calls `done` when
//! it finishes executing. The main task can call `wait` to block until all registered workers are done.
//!
//! # Examples
//!
//! ```rust
//! # fn main() {
//! # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
//! # rt.block_on(async {
//! use awaitgroup::WaitGroup;
//!
//! let mut wg = WaitGroup::new();
//!
//! for _ in 0..5 {
//!     // Create a new worker.
//!     let worker = wg.worker();
//!
//!     tokio::spawn(async {
//!         // Do some work...
//!
//!         // This task is done all of its work.
//!         worker.done();
//!     });
//! }
//!
//! // Block until all other tasks have finished their work.
//! wg.wait().await;
//! # });
//! # }
//! ```
//!
//! A `WaitGroup` can be re-used and awaited multiple times.
//! ```rust
//! # use awaitgroup::WaitGroup;
//! # fn main() {
//! # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
//! # rt.block_on(async {
//! let mut wg = WaitGroup::new();
//!
//! let worker = wg.worker();
//!
//! tokio::spawn(async {
//!     // Do work...
//!     worker.done();
//! });
//!
//! // Wait for tasks to finish
//! wg.wait().await;
//!
//! // Re-use wait group
//! let worker = wg.worker();
//!
//! tokio::spawn(async {
//!     // Do more work...
//!     worker.done();
//! });
//!
//! wg.wait().await;
//! # });
//! # }
//! ```
#![deny(missing_debug_implementations, rust_2018_idioms)]
use anyhow::anyhow;
use anyhow::Result;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Wait for a collection of tasks to finish execution.
///
/// Refer to the [crate level documentation](crate) for details.
#[derive(Clone)]
pub struct WaitGroup {
    inner: Arc<Inner>,
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner::new()),
        }
    }
}

#[allow(clippy::new_without_default)]
impl WaitGroup {
    /// Creates a new `WaitGroup`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new worker.
    pub fn worker(&self) -> Worker {
        Worker {
            inner: self.inner.clone(),
        }
    }

    /// Wait until all registered workers finish executing.
    pub async fn wait(&self) -> Result<()> {
        WaitGroupFuture::new(&self.inner).await
    }

    pub fn count(&self) -> i32 {
        self.inner.count.load(Ordering::Relaxed)
    }

    pub async fn wait_complete(&self, count: usize) -> Result<()> {
        WaitGroupErrorFuture::new(&self.inner, count).await
    }
}

struct WaitGroupFuture<'a> {
    inner: &'a Arc<Inner>,
}

impl<'a> WaitGroupFuture<'a> {
    fn new(inner: &'a Arc<Inner>) -> Self {
        Self { inner }
    }
}

impl Future for WaitGroupFuture<'_> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = cx.waker().clone();
        *self.inner.waker.lock().unwrap() = Some(waker);

        let count = self.inner.count.load(Ordering::Relaxed);
        if count < 0 {
            return Poll::Ready(Err(anyhow!("err:count < 0 => count:{}", count)));
        }

        let error = &mut *self.inner.error.lock().unwrap();
        if error.is_some() {
            return Poll::Ready(Err(anyhow!(
                "err:error => count:{}, err:{}",
                count,
                error.take().unwrap()
            )));
        }

        match count {
            0 => {
                self.inner.complete.store(true, Ordering::SeqCst);
                Poll::Ready(Ok(()))
            }
            _ => Poll::Pending,
        }
    }
}

struct WaitGroupErrorFuture<'a> {
    inner: &'a Arc<Inner>,
    count: usize,
}

impl<'a> WaitGroupErrorFuture<'a> {
    fn new(inner: &'a Arc<Inner>, count: usize) -> Self {
        Self { inner, count }
    }
}

impl Future for WaitGroupErrorFuture<'_> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = cx.waker().clone();
        *self.inner.waker.lock().unwrap() = Some(waker);

        let count = self.inner.count.load(Ordering::Relaxed);
        if count < 0 {
            return Poll::Ready(Err(anyhow!("err:count < 0 => count:{}", count)));
        }

        let error = &mut *self.inner.error.lock().unwrap();
        if error.is_some() {
            return Poll::Ready(Err(anyhow!(
                "err:error => count:{}, err:{}",
                count,
                error.take().unwrap()
            )));
        }

        if count == self.count as i32 {
            self.inner.complete.store(true, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

struct Inner {
    waker: Mutex<Option<Waker>>,
    count: AtomicI32,
    error: Arc<Mutex<Option<anyhow::Error>>>,
    complete: AtomicBool,
}

impl Inner {
    pub fn new() -> Self {
        Self {
            count: AtomicI32::new(0),
            waker: Mutex::new(None),
            error: Arc::new(Mutex::new(None)),
            complete: AtomicBool::new(false),
        }
    }
}

/// A worker registered in a `WaitGroup`.
///
/// Refer to the [crate level documentation](crate) for details.
#[derive(Clone)]
pub struct Worker {
    inner: Arc<Inner>,
}

impl Worker {
    /// Notify the `WaitGroup` that this worker has finished execution.
    pub fn worker(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
    pub fn add(&self) -> WorkerInner {
        if self.inner.complete.load(Ordering::SeqCst) {
            log::error!("complete is true");
        }

        self.inner.count.fetch_add(1, Ordering::Relaxed);
        if let Some(waker) = self.inner.waker.lock().unwrap().take() {
            waker.wake();
        }
        WorkerInner {
            inner: self.inner.clone(),
        }
    }

    pub fn count(&self) -> i32 {
        self.inner.count.load(Ordering::Relaxed)
    }

    pub fn error(&self, err: anyhow::Error) {
        *self.inner.error.lock().unwrap() = Some(err);
        if let Some(waker) = self.inner.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
}

pub struct WorkerInner {
    inner: Arc<Inner>,
}

impl WorkerInner {
    pub fn worker(&self) -> Worker {
        Worker {
            inner: self.inner.clone(),
        }
    }

    pub fn count(&self) -> i32 {
        self.inner.count.load(Ordering::Relaxed)
    }

    pub fn done(&self) {
        if self.inner.complete.load(Ordering::SeqCst) {
            log::error!("complete is true");
        }

        let count = self.inner.count.fetch_sub(1, Ordering::Relaxed);
        if count <= 0 {
            log::error!("count <= 0");
        }
        // We are the last worker
        if count == 1 {
            if let Some(waker) = self.inner.waker.lock().unwrap().take() {
                waker.wake();
            }
        }
    }

    pub fn error(&self, err: anyhow::Error) {
        *self.inner.error.lock().unwrap() = Some(err);
        if let Some(waker) = self.inner.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
}

impl fmt::Debug for WaitGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.inner.count.load(Ordering::Relaxed);
        f.debug_struct("WaitGroup").field("count", &count).finish()
    }
}

impl fmt::Debug for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.inner.count.load(Ordering::Relaxed);
        f.debug_struct("WaitGroup").field("count", &count).finish()
    }
}

impl fmt::Debug for WorkerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.inner.count.load(Ordering::Relaxed);
        f.debug_struct("WaitGroup").field("count", &count).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_wait_group() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async move {
            let mut wg = WaitGroup::new();
            let wk = wg.worker();

            for _ in 0..5 {
                let worker = wk.add();

                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    worker.done();
                });
            }

            wg.wait().await;
        });
    }

    #[test]
    fn test_wait_group_reuse() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut wg = WaitGroup::new();
            let wk = wg.worker();

            for _ in 0..5 {
                let worker = wk.add();

                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    worker.done();
                });
            }

            wg.wait().await;

            let wk = wg.worker();
            let worker = wk.add();

            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                worker.done();
            });

            wg.wait().await;
        });
    }

    #[test]
    fn test_worker_clone() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut wg = WaitGroup::new();
            let wk = wg.worker();

            for _ in 0..5 {
                let worker = wk.new().add();

                tokio::spawn(async move {
                    let nested_worker = worker.add();
                    tokio::spawn(async move {
                        nested_worker.done();
                    });
                    worker.done();
                });
            }

            wg.wait().await;
        });
    }
}
