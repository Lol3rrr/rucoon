use core::{future::Future, time::Duration};

use super::{EntryID, Timer};

/// The Sleep future allows you to suspend execution for a specified amount of Time
///
/// # Note
/// The Timer only starts after the first time being pulled so while this should behave correctly
/// when used inside of a Future/async block in related to the Rest of the Block, it might cause
/// confusion when used at the start as it will then only continue after `time_to_first_pull +
/// sleep_duration` instead of the expected `sleep_duration`
///
/// # Accuracy
/// The Accuracy of this Sleep depends on the Interval of the given Timer, so if the Timer has an
/// interval of 10ms, the Accuracy is also 10ms and cannot be made more accurate, without changing
/// the Timer
pub struct Sleep<'t, const TASKS: usize, const SLOTS: usize> {
    timer: &'t Timer<TASKS, SLOTS>,
    sleep_ms: u128,
    id: Option<EntryID>,
}

impl<'t, const TASKS: usize, const SLOTS: usize> Sleep<'t, TASKS, SLOTS> {
    /// Creates a new Sleep Future that will be Ready after the specified Duration starting from
    /// the Point in time when it was first polled
    pub fn new(timer: &'t Timer<TASKS, SLOTS>, duration: Duration) -> Self {
        Self {
            timer,
            sleep_ms: duration.as_millis(),
            id: None,
        }
    }
}

impl<'t, const TASKS: usize, const SLOTS: usize> Future for Sleep<'t, TASKS, SLOTS> {
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

impl<'t, const TASKS: usize, const SLOTS: usize> Drop for Sleep<'t, TASKS, SLOTS> {
    fn drop(&mut self) {
        match &mut self.id {
            Some(id) => {
                id.clear(self.timer);
            }
            None => {}
        };
    }
}
