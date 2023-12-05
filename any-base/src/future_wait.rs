use crate::typ::ArcMutex;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

pub struct FutureWait {
    flag: ArcMutex<bool>,
    waker: ArcMutex<Waker>,
}

impl Clone for FutureWait {
    fn clone(&self) -> Self {
        FutureWait {
            flag: self.flag.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl FutureWait {
    pub fn new() -> Self {
        FutureWait {
            flag: ArcMutex::new(false),
            waker: ArcMutex::default(),
        }
    }

    pub fn waker(&self) {
        let waker = self.waker.get_option_mut();
        self.flag.set(true);
        if waker.is_some() {
            waker.clone().unwrap().wake();
        }
    }
}

impl Future for FutureWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut flag = self.flag.get_mut();
        if *flag == true {
            *flag = false;
            self.waker.set_nil();
            Poll::Ready(())
        } else {
            if self.waker.is_none() {
                self.waker.set(cx.waker().clone());
            }
            Poll::Pending
        }
    }
}
