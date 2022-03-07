use core::{
    sync::atomic::{AtomicUsize, Ordering},
    task::{RawWaker, RawWakerVTable, Waker},
};

use alloc::{boxed::Box, sync::Arc};

pub struct TestWaker {
    wake_count: Arc<AtomicUsize>,
}

pub unsafe fn clone(_: *const ()) -> RawWaker {
    todo!()
}
pub unsafe fn wake(_: *const ()) {
    todo!()
}
pub unsafe fn wake_by_ref(ptr: *const ()) {
    let inner = unsafe { &*(ptr as *const TestWaker) };
    inner.wake_count.fetch_add(1, Ordering::SeqCst);
}
pub unsafe fn drop(_: *const ()) {
    todo!()
}

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

impl TestWaker {
    pub fn new(wake_count: Arc<AtomicUsize>) -> Self {
        Self { wake_count }
    }

    pub fn into_waker(self) -> Waker {
        let boxed = Box::new(self);
        let ptr = Box::into_raw(boxed);

        unsafe { Waker::from_raw(RawWaker::new(ptr as *const (), &VTABLE)) }
    }
}
