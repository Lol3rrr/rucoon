//! Contains all things related to the actual Runtime
//!
//! # Example
//! ```no_run
//! # use rucoon::Runtime;
//!
//! static RUNTIME: Runtime<10> = Runtime::new();
//!
//! // This is just a placeholder and can be any sort of Future or async Function
//! async fn some_future() {}
//!
//! fn main() {
//!     RUNTIME
//!         .add_task(some_future())
//!         .expect("There should be enough Space for 10 Tasks and this is the first");
//!
//!     RUNTIME.run().expect("We only start the Runtime once here");
//! }
//! ```

use core::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
};

mod task;
use task::Task;

mod tasklist;
pub(crate) use tasklist::TaskList;

mod taskqueue;
pub(crate) use taskqueue::{QueueSender, TaskQueue};

mod waker;
use self::waker::RWaker;

mod executor;
use executor::Executor;

pub mod multithreaded;

/// The ID used to identify a Task
#[derive(Debug, Clone)]
pub struct TaskID(pub(crate) usize);

/// A single threaded Runtime, for more details see the [runtime](crate::runtime) Module Documentation
///
/// The TASKS constant is used to set the maximum Number of Tasks that can be run on this Instance
pub struct Runtime<const TASKS: usize> {
    task_list: TaskList<TASKS>,
    running_tasks: AtomicUsize,
    executor: Executor<TASKS>,
}

/// The Error returned when failing to add a new Task
#[derive(Debug)]
pub enum AddTaskError {
    /// The Runtime is already at maximum Capacity of Tasks
    TooManyTasks,
}

/// The Error returned when failing to run the Runtime itself
#[derive(Debug)]
pub enum RunError {
    /// The Runtime is already running
    AlreadyRunning,
}

impl<const N: usize> Runtime<N> {
    /// Creates a new empty Runtime
    pub const fn new() -> Self {
        Self {
            task_list: TaskList::new(),
            running_tasks: AtomicUsize::new(0),
            executor: Executor::new(0),
        }
    }

    /// Trys to add a new Task to the Runtime, fails in case there are the Runtime already has its
    /// maximum Number of Tasks added to it
    pub fn add_task<F>(&'static self, fut: F) -> Result<TaskID, AddTaskError>
    where
        F: Future + 'static,
    {
        let waker = RWaker::new(self.queue_sender(), TaskID(0));
        let task = Task::new(fut, waker);
        let task_id = match self.task_list.add_task(task) {
            Ok(id) => id,
            Err(_) => return Err(AddTaskError::TooManyTasks),
        };

        let mut task_ref = self.task_list.get_task(task_id.clone()).unwrap();
        task_ref.update_waker(RWaker::new(self.queue_sender(), task_id.clone()));

        if self
            .executor
            .queue
            .sender()
            .enqueue(task_id.clone())
            .is_err()
        {
            return Err(AddTaskError::TooManyTasks);
        }

        self.running_tasks.fetch_add(1, Ordering::SeqCst);

        Ok(task_id)
    }

    /// Returns a handle to a Sender to add Tasks to be polled
    const fn queue_sender(&self) -> QueueSender<'_> {
        self.executor.queue.sender()
    }

    /// Actually starts the Runtime and starts running the Tasks, see
    /// [run_with_sleep](Self::run_with_sleep) for more Details.
    ///
    /// This function is equivilant to calling [run_with_sleep](Self::run_with_sleep) with an empty
    /// sleep function
    pub fn run(&self) -> Result<(), RunError> {
        self.run_with_sleep(|| {})
    }

    /// Starts the Runtime with the given Sleep Function which will be called everytime it tries to
    /// run the next Task without there being any.
    /// This allows you to implement your own sort of Backoff to be more efficient with resources
    /// usage
    ///
    /// # Behaviour
    /// This method will return an Error if the Runtime was already started once, regardless of
    /// whether or not it is actually still running or not.
    /// If this is the first time it is started, it will block the current Thread and run until
    /// every Task added to it has completed and will then return Ok(()) once it is done.
    pub fn run_with_sleep<S>(&self, sleep: S) -> Result<(), RunError>
    where
        S: Fn(),
    {
        self.executor.run(
            sleep,
            &self.task_list,
            &self.running_tasks,
            &[&self.executor],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sample_func() {}

    #[test]
    fn create_new() {
        let _runtime = Runtime::<10>::new();
    }

    #[test]
    fn add_too_many_tasks() {
        static RUNTIME: Runtime<2> = Runtime::<2>::new();

        assert!(RUNTIME.add_task(sample_func()).is_ok());
        assert!(RUNTIME.add_task(sample_func()).is_ok());
        assert!(RUNTIME.add_task(sample_func()).is_err());
    }
}
