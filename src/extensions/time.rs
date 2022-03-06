//! # Purpose
//! This Extension gives you time related Futures, like sleep
//!
//! # Usage
//! To be able to use this Extension you need to create a static Timer Instance, which needs to be
//! supplied to all the other uses in this Module and you also need to run the [Timer::update]
//! Method at the Interval you specified when creating the Timer Instance

use core::{
    cell::UnsafeCell,
    future::Future,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    task::Waker,
};

use crate::internal;

/// The Timer
pub struct Timer<const TASKS: usize> {
    wheel: TimerWheel<TASKS, 128>,
    current_slot: AtomicUsize,
    steps_ms: usize,
}

impl<const TASKS: usize> Timer<TASKS> {
    /// Creates a new Timer Instance
    pub const fn new(interval_ms: usize) -> Self {
        let wheel = TimerWheel::<TASKS, 128>::new();

        Self {
            wheel,
            current_slot: AtomicUsize::new(0),
            steps_ms: interval_ms,
        }
    }

    fn register_timer(&self, target_ms: usize, waker: Waker) -> EntryID {
        let current_slot = self.current_slot.load(Ordering::SeqCst);

        let slot_offset = target_ms / self.steps_ms;
        let target_slot = (current_slot + slot_offset) % 128 - 1;
        let count = (current_slot + slot_offset) / 128 + 1;

        self.wheel.insert_slot(target_slot, count, waker)
    }

    /// Update
    pub fn update(&self) {
        let current_slot = self.current_slot.load(Ordering::SeqCst);
        let next_slot = if current_slot == 127 {
            0
        } else {
            current_slot + 1
        };
        let _ = self.current_slot.compare_exchange(
            current_slot,
            next_slot,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );

        self.wheel.fire_slot(current_slot);
    }
}

#[derive(Debug, PartialEq)]
struct EntryID {
    slot: usize,
    task: usize,
}

impl EntryID {
    pub fn has_fired<const T: usize>(&self, timer: &Timer<T>) -> bool {
        let slot = timer.wheel.slots.get(self.slot).unwrap();
        let task = slot.tasks.get(self.task).unwrap();

        task.fired.load(Ordering::SeqCst)
    }
}

struct WheelEntry {
    status: AtomicU8,
    waker: UnsafeCell<MaybeUninit<Waker>>,
    count: AtomicUsize,
    fired: AtomicBool,
}

unsafe impl Send for WheelEntry {}
unsafe impl Sync for WheelEntry {}

impl WheelEntry {
    pub const fn new() -> Self {
        Self {
            status: AtomicU8::new(0),
            waker: UnsafeCell::new(MaybeUninit::uninit()),
            count: AtomicUsize::new(0),
            fired: AtomicBool::new(false),
        }
    }

    pub fn fire(&self) {
        if self.status.load(Ordering::SeqCst) != 2 {
            return;
        }

        if self.fired.load(Ordering::SeqCst) {
            return;
        }

        let prev_count = self.count.fetch_sub(1, Ordering::SeqCst);
        if prev_count > 1 {
            return;
        }

        if self
            .fired
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let waker_ptr = self.waker.get();
        let waker = unsafe { (&*waker_ptr).assume_init_ref() };

        waker.wake_by_ref();
    }
}

struct WheelSlot<const TASKS: usize> {
    tasks: [WheelEntry; TASKS],
}

impl<const TASKS: usize> WheelSlot<TASKS> {
    pub const fn new() -> Self {
        let tasks = internal::const_array!(TASKS, WheelEntry, WheelEntry::new());

        Self { tasks }
    }

    pub fn insert(&self, count: usize, waker: Waker) -> usize {
        // Find an Entry which does not already contain one and lock it
        let (index, entry) = self
            .tasks
            .iter()
            .enumerate()
            .find(|(_, e)| {
                e.status
                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            })
            .unwrap();

        // Store the Waker into the Entry
        let waker_ptr = entry.waker.get();
        unsafe {
            waker_ptr.write(MaybeUninit::new(waker));
        }

        // Store the Count into the Entry
        entry.count.store(count, Ordering::SeqCst);

        // Reset the Fired flag
        entry.fired.store(false, Ordering::SeqCst);

        // Mark the Entry as being populated and ready
        entry.status.store(2, Ordering::SeqCst);

        index
    }

    pub fn fire(&self) {
        for entry in self.tasks.iter() {
            entry.fire();
        }
    }
}

struct TimerWheel<const TASKS: usize, const SLOTS: usize> {
    slots: [WheelSlot<TASKS>; SLOTS],
}

impl<const TASKS: usize, const SLOTS: usize> TimerWheel<TASKS, SLOTS> {
    pub const fn new() -> Self {
        let slots = internal::const_array!(SLOTS, WheelSlot<TASKS>, WheelSlot::new());

        Self { slots }
    }

    pub fn insert_slot(&self, slot: usize, count: usize, waker: Waker) -> EntryID {
        let slot_entry = self.slots.get(slot).unwrap();

        let task_index = slot_entry.insert(count, waker);

        EntryID {
            slot,
            task: task_index,
        }
    }

    pub fn fire_slot(&self, slot: usize) {
        let slot_entry = self.slots.get(slot).unwrap();

        slot_entry.fire();
    }
}

/// The Sleep future allows you to suspend execution for a specified amount of Time
pub struct Sleep<'t, const TASKS: usize> {
    timer: &'t Timer<TASKS>,
    sleep_ms: usize,
    id: Option<EntryID>,
}

impl<'t, const TASKS: usize> Sleep<'t, TASKS> {
    /// Creates a new Sleep Future
    pub fn new(timer: &'t Timer<TASKS>, sleep_ms: usize) -> Self {
        Self {
            timer,
            sleep_ms,
            id: None,
        }
    }
}

impl<'t, const TASKS: usize> Future for Sleep<'t, TASKS> {
    type Output = ();

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        match &self.id {
            None => {
                let n_id = self.timer.register_timer(self.sleep_ms, cx.waker().clone());

                self.id = Some(n_id);

                core::task::Poll::Pending
            }
            Some(id) => {
                if id.has_fired(self.timer) {
                    core::task::Poll::Ready(())
                } else {
                    todo!("Was pulled without being Fired")
                }
            }
        }
    }
}

impl<'t, const TASKS: usize> Drop for Sleep<'t, TASKS> {
    fn drop(&mut self) {
        // TODO
        // Clear the Entry in the Timer again
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;

    use crate::extensions::testing::TestWaker;

    use super::*;

    #[test]
    fn new_timer() {
        let _ = Timer::<10>::new(10);
    }

    #[test]
    fn timer_register() {
        let counter = Arc::new(AtomicUsize::new(0));

        let waker = TestWaker::new(counter.clone()).into_waker();

        let timer = Timer::<10>::new(10);

        let id = timer.register_timer(30, waker);
        assert_eq!(EntryID { slot: 2, task: 0 }, id);
    }

    #[test]
    fn timer_register_and_fire() {
        let counter = Arc::new(AtomicUsize::new(0));

        let waker = TestWaker::new(counter.clone()).into_waker();

        let timer = Timer::<10>::new(10);

        let id = timer.register_timer(30, waker);

        timer.update();
        timer.update();
        timer.update();

        assert_eq!(1, counter.load(Ordering::SeqCst));
        assert_eq!(true, id.has_fired(&timer));
    }

    #[test]
    fn create_sleep() {
        let timer = Timer::<10>::new(10);

        let _ = Sleep::new(&timer, 100);
    }
}
