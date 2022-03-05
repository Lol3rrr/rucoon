use core::task::{RawWaker, RawWakerVTable};

use super::{QueueSender, TaskID};

pub struct RWaker {
    queue: QueueSender<'static>,
    id: TaskID,
}

impl RWaker {
    pub(crate) fn new(queue: QueueSender<'static>, id: TaskID) -> Self {
        Self { queue, id }
    }

    unsafe fn ptr_ref(raw: *const ()) -> &'static Self {
        let ptr: *const Self = raw as *const Self;
        unsafe { &*ptr }
    }
}

unsafe fn clone(ptr: *const ()) -> RawWaker {
    core::task::RawWaker::new(ptr, &VTABLE)
}
unsafe fn wake(ptr: *const ()) {
    let waker = unsafe { RWaker::ptr_ref(ptr) };
    waker.queue.enqueue(waker.id.clone()).unwrap();
}
unsafe fn wake_by_ref(ptr: *const ()) {
    let waker = unsafe { RWaker::ptr_ref(ptr) };
    waker.queue.enqueue(waker.id.clone()).unwrap();
}
unsafe fn drop(_: *const ()) {}

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

pub fn create_waker(waker_ptr: *const RWaker) -> core::task::Waker {
    let raw = core::task::RawWaker::new(waker_ptr as *const (), &VTABLE);
    unsafe { core::task::Waker::from_raw(raw) }
}
