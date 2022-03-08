use alloc::boxed::Box;
use core::{future::Future, pin::Pin};

use super::waker::RWaker;

pub struct Task {
    fut: Pin<Box<dyn Future<Output = ()>>>,
    waker: *const RWaker,
}

impl Task {
    /// This is simply used to turn any Future into a future that returns nothing, which is needed
    /// to accurately store it internally
    async fn erase<F>(fut: F)
    where
        F: Future,
    {
        fut.await;
    }
    /// Creates a new Task for the given Future and with the given Waker
    ///
    /// # Note
    /// The Waker will be allocated on the Heap as we need to obtain a Ptr to it
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

    /// This will replace the Waker stored in this Task with the new Waker
    pub fn update_waker(&mut self, nwaker: RWaker) {
        // Safety:
        // This should be save because we have a mutable reference to the Task and therefore no
        // other Reference to this should exist making it save to just modify it.
        //
        // We also know that the Pointer is valid because we created it in the new Method and
        // therefore this Write should also not hit any Problems in that regard
        unsafe {
            (self.waker as *mut RWaker).write(nwaker);
        }
    }

    /// Returns a Ptr to the Waker stored for this Task
    pub fn waker_ptr(&self) -> *const RWaker {
        self.waker
    }

    /// Polls the Future of this Task
    pub fn poll(&mut self, ctx: &mut core::task::Context) -> core::task::Poll<()> {
        self.fut.as_mut().poll(ctx)
    }
}
