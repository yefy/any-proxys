#![allow(dead_code, unused_imports)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

#[derive(Clone)]
struct AsyncWait {
    waker: Arc<std::sync::Mutex<Option<Waker>>>,
}

impl AsyncWait {
    pub fn new() -> AsyncWait {
        AsyncWait {
            waker: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn waker(&self) {
        let waker = self.waker.lock().unwrap().take();
        if waker.is_some() {
            waker.unwrap().wake();
        }
    }
}

impl Future for AsyncWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        *self.waker.lock().unwrap() = Some(cx.waker().clone());
        Poll::Pending
    }
}
