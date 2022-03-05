use alloc::boxed::Box;
use core::{future::Future, pin::Pin};

use super::waker::RWaker;

pub struct Task {
    fut: Pin<Box<dyn Future<Output = ()>>>,
    waker: *const RWaker,
}

impl Task {
    async fn erase<F>(fut: F)
    where
        F: Future,
    {
        fut.await;
    }
    pub fn new<F>(fut: F, waker: RWaker) -> Self
    where
        F: Future + 'static,
    {
        let boxed_waker = Box::new(waker);
        let ptr = Box::into_raw(boxed_waker);

        Self {
            fut: Box::pin(Self::erase(fut)),
            waker: ptr,
        }
    }

    pub fn update_waker(&mut self, nwaker: RWaker) {
        unsafe {
            (self.waker as *mut RWaker).write(nwaker);
        }
    }
    pub fn waker_ptr(&self) -> *const RWaker {
        self.waker
    }

    pub fn poll(&mut self, ctx: &mut core::task::Context) -> core::task::Poll<()> {
        self.fut.as_mut().poll(ctx)
    }
}
