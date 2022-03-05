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
