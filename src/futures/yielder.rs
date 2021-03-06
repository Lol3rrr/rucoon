use core::future::Future;

/// This will immediately yield to the Runtime, allowing you to force a yield in a long running
/// Computation to allow the Runtime to also run other Tasks
pub struct Yielder {
    polled: bool,
}

impl Yielder {
    /// Creates a new Yielder Instance to be awaited
    pub fn new() -> Self {
        Self { polled: false }
    }
}

impl Default for Yielder {
    fn default() -> Self {
        Self::new()
    }
}

impl Future for Yielder {
    type Output = ();

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        if !self.polled {
            self.polled = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        } else {
            core::task::Poll::Ready(())
        }
    }
}

/// Creates a future that will yield once when polled. This enables you to insert yield
/// points in long running computations, to allow other Futures to be run between these
/// Points and avoid starving the Runtime.
///
/// Note: This is a simple Wrapper around [Yielder::new] to make it more ergonomic to use
/// and it's also more familiar to await a function call like this.
pub fn yield_now() -> Yielder {
    Yielder::new()
}
