//! Contains the Executor that is actually responsible for running the Futures/Tasks

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::{waker, RunError, TaskList, TaskQueue};

/// An Executor with its own Queue of Tasks to run
pub struct Executor<const TASKS: usize> {
    id: usize,
    pub(crate) queue: TaskQueue<TASKS>,
    running: AtomicBool,
}

impl<const TASKS: usize> Executor<TASKS> {
    pub const fn new(id: usize) -> Self {
        Self {
            id,
            queue: TaskQueue::new(),
            running: AtomicBool::new(false),
        }
    }

    pub const fn set_id(&mut self, n_id: usize) {
        self.id = n_id;
    }

    /// Actually starts executing the Tasks in its Queue
    pub fn run<F>(
        &self,
        sleep: F,
        tasks: &TaskList<TASKS>,
        running_tasks: &AtomicUsize,
        executors: &[&Self],
    ) -> Result<(), RunError>
    where
        F: Fn(),
    {
        // If the Executor was already started once, we will return this Error
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(RunError::AlreadyRunning);
        }

        // Iterates over the Executors endlessly
        let mut steal_iter = executors.iter().cycle();

        // The Main execution loop
        loop {
            // Obtain the ID of the next Task to execute
            let task_id = match self.queue.dequeue() {
                Some(t) => t,
                None => {
                    // If there is no Task in the Queue and there are no more running Tasks, simply
                    // return
                    if running_tasks.load(Ordering::Relaxed) == 0 {
                        return Ok(());
                    }

                    // An Iterator over all the other Executors
                    let mut steal_iter = steal_iter
                        .by_ref()
                        .take(executors.len())
                        .filter(|e| e.id != self.id);

                    // Try to steal a Task from the Queue of another Executor and use that one if
                    // possible
                    match steal_iter.find_map(|e| e.queue.dequeue()) {
                        Some(task) => task,
                        None => {
                            // There are running Tasks, but nothing in the Queue so we will sleep and retry
                            // it later
                            sleep();
                            continue;
                        }
                    }
                }
            };

            // Get the TaskGuard for the Task we want to execute
            let mut task = match tasks.get_task(task_id) {
                Some(t) => t,
                None => continue,
            };

            // Create the correct async Context
            let waker = waker::create_waker(task.waker_ptr());
            let mut ctx = core::task::Context::from_waker(&waker);

            // Actually execute the Future
            match task.poll(&mut ctx) {
                core::task::Poll::Ready(_) => {
                    // If we just ran the last Future to completion we will return as there is
                    // nothing more to do for us
                    let prev = running_tasks.fetch_sub(1, Ordering::SeqCst);
                    if prev == 1 {
                        return Ok(());
                    }
                }
                core::task::Poll::Pending => {}
            };
        }
    }
}
