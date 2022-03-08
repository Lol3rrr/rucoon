use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

use crate::internal;

use super::{Task, TaskID};

mod status;
use status::{AtomicStatus, SlotStatus};

pub struct TaskList<const N: usize> {
    slots: [TaskSlot; N],
}

struct TaskSlot {
    status: AtomicStatus,
    task: UnsafeCell<MaybeUninit<Task>>,
}

unsafe impl<const N: usize> Send for TaskList<N> {}
unsafe impl<const N: usize> Sync for TaskList<N> {}

impl<const N: usize> TaskList<N> {
    pub const fn new() -> Self {
        let slots = internal::const_array!(
            N,
            TaskSlot,
            TaskSlot {
                status: AtomicStatus::new(SlotStatus::Empty),
                task: UnsafeCell::new(MaybeUninit::uninit()),
            }
        );

        Self { slots }
    }

    pub(crate) fn add_task(&self, task: Task) -> Result<TaskID, ()> {
        // Search for a free Slot by trying to lock each one, this is definetly not the best Method
        // because we cause a lot of contention on these Atomics in the CPU itself, but it works
        // for now and it can always be changed later if it does become a bigger Problem
        let (index, slot) = self
            .slots
            .iter()
            .enumerate()
            .find(|(_, slot)| {
                slot.status
                    .compare_exchange(
                        SlotStatus::Empty,
                        SlotStatus::Locked,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
            })
            // If we can't lock a Slot for the Task, we just return an Error indicating that no
            // free Space is currently available
            .ok_or(())?;

        // The Slot is now locked by us and therefore we have exclusive access to its Data

        let data_ptr = slot.task.get();
        // Safety:
        // This is safe because as said previously, we now have exclusive access to the Data in the
        // Slot and therefore we can safely store the task we want to add into the Cell
        unsafe {
            data_ptr.write(MaybeUninit::new(task));
        }

        // We have now setup the Slot correctly and it can be used by the Rest of the Runtime so we
        // set it's Status to Set indicating that the Data in it is valid and can be used
        slot.status.store(SlotStatus::Set, Ordering::SeqCst);

        Ok(TaskID(index))
    }

    pub(crate) fn get_task(&self, id: TaskID) -> Option<TaskGuard<'_>> {
        let index = id.0;

        let slot = self.slots.get(index)?;

        TaskGuard::obtain(slot)
    }
}

/// The TaskGuard allows you access a single Task in the List, the List itself allows only one
/// TaskGaurd to exist per Task, so every TaskGuard is exclusive
pub struct TaskGuard<'t> {
    task: &'t TaskSlot,
}

impl<'t> TaskGuard<'t> {
    /// This is used to obtain a TaskGuard for the given Slot.
    ///
    /// This fails if the Slot has a different Status than Set, because that means it is either
    /// Empty, Locked or currently being accessed by a different TaskGuard, all of which means we
    /// can not safely access this Slot with a new TaskGuard
    fn obtain(slot: &'t TaskSlot) -> Option<Self> {
        // Try to mark the Slot as Accessed, abort if this fails as that means the Slot is already
        // being used by someone else
        if slot
            .status
            .compare_exchange(
                SlotStatus::Set,
                SlotStatus::Accessed,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return None;
        }

        Some(Self { task: slot })
    }
}

impl<'t> Drop for TaskGuard<'t> {
    fn drop(&mut self) {
        // We mark the underlying Task as SetButFree again and therefore making sure that we can
        // get a new TaskGuard for this Task in the Future again
        self.task.status.store(SlotStatus::Set, Ordering::SeqCst);
    }
}

impl<'t> Deref for TaskGuard<'t> {
    type Target = Task;

    fn deref(&self) -> &Self::Target {
        let data_ptr = self.task.task.get();
        // Safety
        // This should be save because we have exclusive access to this Slot and therefor noone
        // else can access this Data while the TaskGuard still exists
        unsafe { (&*data_ptr).assume_init_ref() }
    }
}
impl<'t> DerefMut for TaskGuard<'t> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let data_ptr = self.task.task.get();
        // Safety
        // This should be save because we have exclusive access to this Slot and therefor noone
        // else can access this Data while the TaskGuard still exists
        unsafe { (&mut *data_ptr).assume_init_mut() }
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::{waker::RWaker, Runtime};

    use super::*;

    async fn example_func() {}

    #[test]
    fn create_list() {
        let _ = TaskList::<10>::new();
    }

    #[test]
    fn add_get_task() {
        static RUNTIME: Runtime<10> = Runtime::new();
        let list = TaskList::<1>::new();

        let id_res = list.add_task(Task::new(
            example_func(),
            RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
        ));

        assert!(id_res.is_ok());
        let id = id_res.unwrap();
        assert_eq!(0, id.0);

        let guard_res = list.get_task(id.clone());
        assert!(guard_res.is_some());
    }

    #[test]
    fn add_multiple_tasks() {
        static RUNTIME: Runtime<10> = Runtime::new();
        let list = TaskList::<3>::new();

        assert!(list
            .add_task(Task::new(
                example_func(),
                RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
            ))
            .is_ok());
        assert!(list
            .add_task(Task::new(
                example_func(),
                RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
            ))
            .is_ok());
        assert!(list
            .add_task(Task::new(
                example_func(),
                RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
            ))
            .is_ok());
    }

    #[test]
    fn add_to_many_tasks() {
        static RUNTIME: Runtime<10> = Runtime::new();
        let list = TaskList::<1>::new();

        assert!(list
            .add_task(Task::new(
                example_func(),
                RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
            ))
            .is_ok());
        assert!(list
            .add_task(Task::new(
                example_func(),
                RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
            ))
            .is_err());
    }

    #[test]
    fn get_multiple_guards() {
        static RUNTIME: Runtime<10> = Runtime::new();
        let list = TaskList::<1>::new();

        let id_res = list.add_task(Task::new(
            example_func(),
            RWaker::new(RUNTIME.queue_sender(), TaskID(0)),
        ));

        assert!(id_res.is_ok());
        let id = id_res.unwrap();
        assert_eq!(0, id.0);

        let first_guard_res = list.get_task(id.clone());
        assert!(first_guard_res.is_some());

        let second_guard_res = list.get_task(id.clone());
        assert!(second_guard_res.is_none());

        drop(first_guard_res);

        let third_guard_res = list.get_task(id.clone());
        assert!(third_guard_res.is_some());
    }
}
