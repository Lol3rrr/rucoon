//! # Purpose
//! This Extension gives you time related Futures, like sleep
//!
//! # Usage
//! To be able to use this Extension you need to create a static Timer Instance, which needs to be
//! supplied to all the other uses in this Module and you also need to run the [Timer::update]
//! Method at the Interval you specified when creating the Timer Instance.
//!
//! # Accuracy
//! The Interval of the Timer also determines the Accuracy of the Operations performed on it, so
//! with an Interval of 10ms the Accuracy would also be 10ms.
//! Timers are also rounded down, so again with an Interval of 10ms the Timers for 10ms, 12ms, 15ms
//! and 19ms would all be triggered at the same Time, as they all fall into the same "Bucket".
//!
//! # Example
//! ```no_run
//! # use rucoon::Runtime;
//! # use rucoon::extensions::time::{Timer, Sleep};
//! # use std::time::{Instant, Duration};
//!
//! static RUNTIME: Runtime<10> = Runtime::new();
//! static TIMER: Timer<10> = Timer::new(10);
//!
//! async fn func_with_sleep() {
//!     println!("Before Sleep");
//!
//!     Sleep::new(&TIMER, Duration::from_millis(50));
//!
//!     println!("After Sleep");
//! }
//!
//! fn main() {
//!     RUNTIME.add_task(func_with_sleep());
//!
//!     // This thread will be responsible for providing the "Clock Ticks" to actually advance the
//!     // Timer and make it work
//!     std::thread::spawn(|| {
//!         let interval_time = Duration::from_millis(10);
//!
//!         loop {
//!             let start = Instant::now();
//!
//!             TIMER.update();
//!
//!             let sleep_time = interval_time.saturating_sub(start.elapsed());
//!             std::thread::sleep(sleep_time);
//!         }
//!     });
//!
//!     RUNTIME.run().unwrap();
//! }
//! ```

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    task::Waker,
};

use crate::internal;

mod sleep;
pub use sleep::Sleep;

/// The Timer that is actually used to keep track of all time related Futures or related Tasks
pub struct Timer<const TASKS: usize, const SLOTS: usize = 128> {
    wheel: TimerWheel<TASKS, SLOTS>,
    current_slot: AtomicUsize,
    steps_ms: u128,
}

impl<const TASKS: usize, const SLOTS: usize> Timer<TASKS, SLOTS> {
    /// Creates a new Timer Instance, which should be updated every `interval_ms` milliseconds to
    /// provide a sort of clock signal for this extensions to work with
    pub const fn new(interval_ms: u128) -> Self {
        let wheel = TimerWheel::<TASKS, SLOTS>::new();

        Self {
            wheel,
            current_slot: AtomicUsize::new(0),
            steps_ms: interval_ms,
        }
    }

    fn register_timer(&self, target_ms: u128, waker: Waker) -> EntryID {
        let current_slot = self.current_slot.load(Ordering::SeqCst) as u128;

        let slot_offset = target_ms / self.steps_ms - 1;
        let target_slot = (current_slot + slot_offset) % SLOTS as u128;
        let count = slot_offset / SLOTS as u128 + 1;

        self.wheel
            .insert_slot(target_slot as usize, count as usize, waker)
    }

    /// This method should be called at the Interval specified when creating this Timer, this will
    /// then handle all actual Timer related work of dispatching correct futures and keeping track
    /// of Time
    pub fn update(&self) {
        let current_slot = self.current_slot.load(Ordering::SeqCst);
        let next_slot = (current_slot + 1) % SLOTS;

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
    pub fn has_fired<const T: usize, const S: usize>(&self, timer: &Timer<T, S>) -> bool {
        let slot = timer.wheel.slots.get(self.slot).unwrap();
        let task = slot.tasks.get(self.task).unwrap();

        task.fired.load(Ordering::SeqCst)
    }

    pub fn clear<const T: usize, const S: usize>(&mut self, timer: &Timer<T, S>) {
        let slot = timer.wheel.slots.get(self.slot).unwrap();
        let task = slot.tasks.get(self.task).unwrap();

        if task
            .status
            .compare_exchange(2, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        task.fired.store(false, Ordering::SeqCst);
        task.count.store(0, Ordering::SeqCst);

        let waker_ptr = task.waker.get();
        let _ = unsafe { waker_ptr.replace(MaybeUninit::uninit()) };

        task.status.store(0, Ordering::SeqCst);
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

/// This is a simply a convient wrapper function for [Sleep::new] as it feels more natural to call
/// a Function and then await that, instead of creating a new Sleep Instance and then using await
/// on that Instance, although they are both doing the same thing
pub fn sleep<const T: usize, const S: usize>(
    timer: &'_ Timer<T, S>,
    duration: core::time::Duration,
) -> Sleep<'_, T, S> {
    Sleep::new(timer, duration)
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

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

        let _ = Sleep::new(&timer, Duration::from_millis(100));
    }
}
