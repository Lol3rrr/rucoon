//! A simple Multithreaded Runtime
//!
//! # Usage
//! The Runtime itself does not provide a direct way for you to run it directly, but instead
//! provides you with an Iterator of Closures that you need to run somehow. This might seem
//! restrictiv and unnecessary but allows you to use this Runtime in a wide variety of Environments
//! without any underlying changes, for example by spawning a Thread for every Closure or
//! distributing them to actual Cores when writing an Operating System or working in embedded.
//!
//! # Example
//! ```no_run
//! # use rucoon::runtime::multithreaded::{MultithreadedRuntime};
//!
//! static RUNTIME: MultithreadedRuntime<10, 2> = MultithreadedRuntime::new();
//!
//! async fn some_task() {
//!     println!("Executed Task");
//! }
//!
//! fn main() {
//!     RUNTIME.add_task(some_task());
//!
//!     let threads: Vec<_> = RUNTIME.run_iter().map(|f| std::thread::spawn(f)).collect();
//!
//!     for handle in threads {
//!         handle.join().unwrap();
//!     }
//! }
//! ```

use core::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::internal;

use super::{
    executor::Executor, task::Task, waker::RWaker, AddTaskError, RunError, TaskID, TaskList,
};

/// A simple Multithreaded Runtime, for more details see
/// [multithreaded](crate::runtime::multithreaded) Module Documentation
pub struct MultithreadedRuntime<const TASKS: usize, const THREADS: usize> {
    task_list: TaskList<TASKS>,
    running_tasks: AtomicUsize,
    executors: [Executor<TASKS>; THREADS],
    enqueue_exec: AtomicUsize,
}

impl<const TASKS: usize, const THREADS: usize> MultithreadedRuntime<TASKS, THREADS> {
    /// Creates a new Instance of the Runtime
    pub const fn new() -> Self {
        let mut executors: [Executor<TASKS>; THREADS] =
            internal::const_array!(THREADS, Executor<TASKS>, Executor::new(0));

        let mut i = 0;
        while i < THREADS {
            executors[i].set_id(i);
            i += 1;
        }

        Self {
            task_list: TaskList::new(),
            running_tasks: AtomicUsize::new(0),
            executors,
            enqueue_exec: AtomicUsize::new(0),
        }
    }

    /// Attempts to add a new Task to the Runtime
    pub fn add_task<F>(&'static self, fut: F) -> Result<TaskID, AddTaskError>
    where
        F: Future + 'static,
    {
        let tmp_exec_pos = self.enqueue_exec.fetch_add(1, Ordering::SeqCst) % THREADS;
        let tmp_exec = self.executors.get(tmp_exec_pos).unwrap();

        let waker = RWaker::new(tmp_exec.queue.sender(), TaskID(0));
        let task = Task::new(fut, waker);
        let task_id = match self.task_list.add_task(task) {
            Ok(id) => id,
            Err(_) => return Err(AddTaskError::TooManyTasks),
        };

        let mut task_ref = self.task_list.get_task(task_id.clone()).unwrap();
        task_ref.update_waker(RWaker::new(tmp_exec.queue.sender(), task_id.clone()));

        if tmp_exec.queue.sender().enqueue(task_id.clone()).is_err() {
            return Err(AddTaskError::TooManyTasks);
        }

        self.running_tasks.fetch_add(1, Ordering::SeqCst);

        Ok(task_id)
    }

    /// Creates an iterator of Closures for you to run independantely. Each Closure corresponds to
    /// a single Thread for the Runtime, which allows you to decide how exactly these are run in
    /// your environment.
    ///
    /// # Note
    /// The Length of this Iterator is Equal to the THREADS constant on the Runtime
    pub fn run_iter(
        &'static self,
    ) -> impl Iterator<Item = impl Fn() -> Result<(), RunError>> + 'static {
        self.executors.iter().map(move |exec| {
            let mut other_execs: [&Executor<TASKS>; THREADS] =
                internal::const_array!(THREADS, &Executor<TASKS>, exec);
            for (i, o_exec) in self.executors.iter().enumerate() {
                other_execs[i] = o_exec;
            }

            move || exec.run(|| {}, &self.task_list, &self.running_tasks, &other_execs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_instance() {
        static _CRUNTIME: MultithreadedRuntime<10, 2> = MultithreadedRuntime::new();
    }

    #[test]
    fn run_iter_length() {
        static RUNTIME2: MultithreadedRuntime<10, 2> = MultithreadedRuntime::new();
        assert_eq!(2, RUNTIME2.run_iter().fold(0, |c, _| c + 1));
    }
}
