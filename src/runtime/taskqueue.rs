use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use crate::internal;

use super::TaskID;

/// The Queue used for Tasks
///
/// This represents both a single Sender and Receiver
pub struct TaskQueue<const N: usize> {
    slots: [QueueSlot; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

/// A Sender for the Task Queue
pub struct QueueSender<'s> {
    slots: &'s [QueueSlot],
    tail: &'s AtomicUsize,
}

#[derive(Debug)]
struct QueueSlot {
    status: AtomicU8,
    data: UnsafeCell<MaybeUninit<TaskID>>,
}

unsafe impl<const N: usize> Send for TaskQueue<N> {}
unsafe impl<const N: usize> Sync for TaskQueue<N> {}

unsafe impl<'s> Send for QueueSender<'s> {}
unsafe impl<'s> Sync for QueueSender<'s> {}

impl<const N: usize> TaskQueue<N> {
    /// Creates a new TaskQueue
    pub const fn new() -> Self {
        let slots = internal::const_array!(
            N,
            QueueSlot,
            QueueSlot {
                status: AtomicU8::new(0),
                data: UnsafeCell::new(MaybeUninit::uninit()),
            }
        );

        Self {
            slots,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Obtains a Sender for the same Queue to allow multiple Senders
    pub const fn sender(&self) -> QueueSender<'_> {
        QueueSender {
            slots: &self.slots,
            tail: &self.tail,
        }
    }

    /// Attempts to dequeue a TaskID once
    fn try_dequeue(&self) -> Option<TaskID> {
        let head = self.head.load(Ordering::SeqCst);
        let next_head = (head + 1) % N;

        let slot = self.slots.get(head).unwrap();
        match slot
            .status
            .compare_exchange(2, 1, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => {}
            Err(0) => return None,
            Err(1) => return None,
            Err(2) => unreachable!(),
            Err(_) => unreachable!(),
        };

        let _ = self
            .head
            .compare_exchange(head, next_head, Ordering::SeqCst, Ordering::SeqCst);

        let data_ptr = slot.data.get();
        let data = unsafe { data_ptr.replace(MaybeUninit::uninit()).assume_init() };

        slot.status.store(0, Ordering::SeqCst);

        Some(data)
    }

    pub(crate) fn dequeue(&self) -> Option<TaskID> {
        for _ in 0..5 {
            if let Some(id) = self.try_dequeue() {
                return Some(id);
            }
        }

        None
    }
}

impl<'s> QueueSender<'s> {
    fn try_enqueue(&self, id: TaskID) -> Result<(), TaskID> {
        let current_tail = self.tail.load(Ordering::SeqCst);
        let next_tail = (current_tail + 1) % self.slots.len();

        let slot = self.slots.get(current_tail).unwrap();
        match slot
            .status
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => {}
            Err(1) => return Err(id),
            Err(2) => return Err(id),
            _ => unreachable!(),
        };

        let _ =
            self.tail
                .compare_exchange(current_tail, next_tail, Ordering::SeqCst, Ordering::SeqCst);

        let data_ptr = slot.data.get();
        unsafe {
            data_ptr.write(MaybeUninit::new(id));
        }
        slot.status.store(2, Ordering::SeqCst);

        Ok(())
    }

    /// Enqueues the TaskID on the Queue
    pub fn enqueue(&self, mut id: TaskID) -> Result<(), TaskID> {
        for _ in 0..5 {
            match self.try_enqueue(id) {
                Ok(_) => return Ok(()),
                Err(i) => {
                    id = i;
                }
            };
        }

        Err(id)
    }
}
