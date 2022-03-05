use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU8, Ordering},
};

use crate::internal;

use super::{Task, TaskID};

pub struct TaskList<const N: usize> {
    slots: [TaskSlot; N],
}

struct TaskSlot {
    status: AtomicU8,
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
                status: AtomicU8::new(0),
                task: UnsafeCell::new(MaybeUninit::uninit()),
            }
        );

        Self { slots }
    }

    pub(crate) fn add_task(&self, task: Task) -> Result<TaskID, ()> {
        for (index, slot) in self.slots.iter().enumerate() {
            if slot
                .status
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }

            let data_ptr = slot.task.get();
            unsafe {
                data_ptr.write(MaybeUninit::new(task));
            }

            slot.status.store(2, Ordering::SeqCst);

            return Ok(TaskID(index));
        }

        Err(())
    }

    pub(crate) fn get_task(&self, id: TaskID) -> Option<TaskGuard<'_>> {
        let index = id.0;

        let slot = self.slots.get(index)?;

        if slot
            .status
            .compare_exchange(2, 3, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }

        Some(TaskGuard { task: slot })
    }
}

pub struct TaskGuard<'t> {
    task: &'t TaskSlot,
}

impl<'t> Drop for TaskGuard<'t> {
    fn drop(&mut self) {
        self.task.status.store(2, Ordering::SeqCst);
    }
}

impl<'t> Deref for TaskGuard<'t> {
    type Target = Task;

    fn deref(&self) -> &Self::Target {
        let data_ptr = self.task.task.get();
        unsafe { (&*data_ptr).assume_init_ref() }
    }
}
impl<'t> DerefMut for TaskGuard<'t> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let data_ptr = self.task.task.get();
        unsafe { (&mut *data_ptr).assume_init_mut() }
    }
}
