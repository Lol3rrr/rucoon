//! Contains all things related to the actual Runtime

use core::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
};

mod task;
use task::Task;

mod tasklist;
pub(crate) use tasklist::TaskList;

mod taskqueue;
pub use taskqueue::QueueSender;
pub(crate) use taskqueue::TaskQueue;

mod waker;
use self::waker::RWaker;

/// An ID used for a single Task
#[derive(Debug, Clone)]
pub struct TaskID(pub usize);

/// The Runtime itself
///
/// The TASKS constant is used to set the maximum Number of Tasks that can be run on this Instance
/// of the Runtime
pub struct Runtime<const TASKS: usize> {
    queue: TaskQueue<TASKS>,
    task_list: TaskList<TASKS>,
    running: AtomicBool,
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
    /// Creates a new Runtime
    pub const fn new() -> Self {
        Self {
            queue: TaskQueue::new(),
            task_list: TaskList::new(),
            running: AtomicBool::new(false),
        }
    }

    /// Adds a new Task to the Runtime to be executed
    pub fn add_task<F>(&'static self, fut: F) -> Result<TaskID, AddTaskError>
    where
        F: Future + 'static,
    {
        let waker = RWaker::new(self.queue_sender(), TaskID(0));
        let task = Task::new(fut, waker);
        let task_id = self.task_list.add_task(task).unwrap();

        let mut task_ref = self.task_list.get_task(task_id.clone()).unwrap();
        task_ref.update_waker(RWaker::new(self.queue_sender(), task_id.clone()));

        if self.queue.sender().enqueue(task_id.clone()).is_err() {
            return Err(AddTaskError::TooManyTasks);
        }

        Ok(task_id)
    }

    /// Returns a handle to a Sender to add Tasks to be executed
    pub const fn queue_sender(&self) -> QueueSender<'_> {
        self.queue.sender()
    }

    /// Actually starts the Runtime and starts running the Tasks
    pub fn run(&self) -> Result<(), RunError> {
        self.run_with_sleep(|| {})
    }

    /// Starts the Runtime with the given Sleep Function which will be called everytime it tries to
    /// run the next Task without there being any.
    /// This allows you to implement your own sort of Backoff to be more efficient with resources
    /// usage
    pub fn run_with_sleep<S>(&self, mut sleep: S) -> Result<(), RunError>
    where
        S: FnMut(),
    {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(RunError::AlreadyRunning);
        }

        loop {
            let task_id = match self.queue.dequeue() {
                Some(t) => t,
                None => {
                    sleep();
                    continue;
                }
            };

            let mut task = self.task_list.get_task(task_id).unwrap();

            let waker = waker::create_waker(task.waker_ptr());
            let mut ctx = core::task::Context::from_waker(&waker);

            match task.poll(&mut ctx) {
                core::task::Poll::Ready(_) => {
                    // todo!("Task is ready");
                }
                core::task::Poll::Pending => {
                    // todo!("Task is pending");
                }
            };
        }
    }
}
