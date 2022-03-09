use super::TaskID;

mod mpmc;

/// The Queue used for Tasks
///
/// This represents both a single Sender and Receiver
pub struct TaskQueue<const N: usize>(mpmc::Queue<TaskID, N>);

/// A Sender for the Task Queue
pub struct QueueSender<'s>(mpmc::Sender<'s, TaskID>);

impl<const N: usize> TaskQueue<N> {
    /// Creates a new TaskQueue
    pub const fn new() -> Self {
        Self(mpmc::Queue::new())
    }

    /// Obtains a Sender for the same Queue to allow multiple Senders
    pub const fn sender(&self) -> QueueSender<'_> {
        QueueSender(self.0.sender())
    }

    pub(crate) fn dequeue(&self) -> Option<TaskID> {
        self.0.dequeue().ok()
    }
}

impl<'s> QueueSender<'s> {
    /// Enqueues the TaskID on the Queue
    pub fn enqueue(&self, id: TaskID) -> Result<(), TaskID> {
        self.0.enqueue(id)
    }
}
