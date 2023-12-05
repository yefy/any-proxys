#![allow(dead_code, unused_imports)]

use crate::typ::ArcMutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

#[derive(Clone)]
struct AsyncWait {
    waker: ArcMutex<Waker>,
}

impl AsyncWait {
    pub fn new() -> AsyncWait {
        AsyncWait {
            waker: ArcMutex::default(),
        }
    }

    pub fn waker(&self) {
        if self.waker.is_some() {
            unsafe { self.waker.take() }.wake();
        }
    }
}

impl Future for AsyncWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        {
            self.waker.set(cx.waker().clone());
        }
        Poll::Pending
    }
}
