//! The runtime itself: the thing that owns the threads and runs your futures.
//!
//! Three pieces live here:
//!
//! - [`Runtime`] - what a user creates, and the only way in.
//! - [`Handle`] - a cheap clone of "the currently running runtime", stashed in
//!   a thread-local so [`spawn`] and [`sleep`](crate::sleep) can find it
//!   without being passed around.
//! - [`run_reactor`] - the loop on the reactor thread that waits for sockets
//!   and timers.

#![allow(dead_code)]

mod handle;
mod shared;
mod worker;
use crate::channel::oneshot;
use crate::runtime::worker::run_worker;
use crate::task::{JoinHandle, Task, catch_unwind};
pub use handle::Handle;
use shared::Shared;
use std::{io, sync::Arc, task::Waker, time::Instant};

/// The async runtime: create one, then hand it work with
/// [`block_on`](Runtime::block_on).
///
/// Everything a running program needs - the queue of work waiting to run, the
/// list of timers, the socket watcher - lives behind this one value.
///
/// ```
/// use nezuko::Runtime;
///
/// let rt = Runtime::new().unwrap();
/// let answer = rt.block_on(async { 6 * 7 });
/// assert_eq!(answer, 42);
/// ```
pub struct Runtime {
    handle: Handle,
}

impl Runtime {
    /// Set up a new runtime.
    ///
    /// No threads start yet - that happens on the first
    /// [`block_on`](Runtime::block_on). This only opens the pipe the reactor
    /// uses to wake itself, which is why it can fail with an I/O error.
    ///
    /// ```
    /// let rt = nezuko::Runtime::new().unwrap();
    /// ```
    pub fn new() -> io::Result<Self> {
        let shared = Arc::new(Shared::new()?);
        Ok(Runtime {
            handle: Handle::new(shared),
        })
    }

    /// Run `future` to completion and give back whatever it returns.
    ///
    /// This is the bridge between ordinary blocking code and async code: the
    /// calling thread parks here until the future is done. Anything the future
    /// [`spawn`]s runs alongside it, on the runtime's own threads.
    ///
    /// ```
    /// use nezuko::{Runtime, spawn};
    ///
    /// let rt = Runtime::new().unwrap();
    ///
    /// let total = rt.block_on(async {
    ///     let a = spawn(async { 20 });
    ///     let b = spawn(async { 22 });
    ///     a.await + b.await
    /// });
    ///
    /// assert_eq!(total, 42);
    /// ```
    ///
    /// # Panics
    ///
    /// If `future` panics, the panic is carried back here and raised on the
    /// calling thread, so it looks the same as if you had run the code
    /// normally.
    ///
    /// # For maintainers
    ///
    /// The order of operations is: mark this thread as "inside the runtime",
    /// start the worker threads and the reactor thread, then push `future`
    /// onto the queue like any other task. The only special thing about it is
    /// the `oneshot` channel wrapped around it, which is how the result
    /// travels from whichever worker finishes it back to this thread.
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let _guard = self.handle.enter();

        // Fixed for now; could be std::thread::available_parallelism().
        let num_workers = 3;
        for _ in 0..num_workers {
            let handle = self.handle.clone();
            let queue = self.handle.shared().queue.clone();
            std::thread::spawn(move || run_worker(handle, queue));
        }

        {
            let shared = self.handle.shared().clone();
            std::thread::spawn(move || run_reactor(shared));
        }

        let (tx, rx) = oneshot::channel();

        let wrapped = async move {
            // Send the panic rather than unwinding on a worker thread, which
            // would leave `recv_blocking` below waiting forever.
            tx.send(catch_unwind(future).await);
        };

        Task::spawn(Box::pin(wrapped), &self.handle.shared().queue);

        match rx.recv_blocking() {
            Ok(output) => output,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
}

/// Start `future` running in the background, right now.
///
/// You get back a handle you can `.await` later to collect the result. If you
/// never await it the task still runs - the handle is just how you pick the
/// value up.
///
/// ```
/// use nezuko::{Runtime, spawn};
///
/// let rt = Runtime::new().unwrap();
///
/// rt.block_on(async {
///     let job = spawn(async { "done" });
///     // ... do other things here ...
///     assert_eq!(job.await, "done");
/// });
/// ```
///
/// # Panics
///
/// Panics if called from a thread that is not running inside a runtime, i.e.
/// outside of [`Runtime::block_on`]. If the spawned task panics, the panic is
/// re-raised when you await its handle.
pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Handle::current().spawn(future)
}

/// The reactor loop - the thread that does the waiting so no one else has to.
///
/// It sleeps inside a single `poll()` call until one of three things happens:
/// a watched socket becomes ready, the nearest timer comes due, or someone
/// pokes the wakeup pipe because the set of things to wait for just changed.
/// Then it wakes whichever tasks were waiting on that and goes back to sleep.
///
/// Runs forever; it is started on its own thread by
/// [`block_on`](Runtime::block_on) and dies with the process.
pub fn run_reactor(shared: Arc<Shared>) {
    loop {
        // How long we may sleep: until the earliest timer, or forever if there
        // are none. wake_times is a BTreeMap, so the first key is the soonest.
        let timeout_ms = {
            let wake_times = shared.wake_times.lock().unwrap();
            if let Some(&next) = wake_times.keys().next() {
                let dur = next.saturating_duration_since(Instant::now());
                // round up: truncating a sub-ms wait to 0 would spin
                dur.as_micros().div_ceil(1000).min(libc::c_int::MAX as u128) as libc::c_int
            } else {
                -1 // no timer .. block forever
            }
        };

        shared
            .reactor
            .poll_and_wake(timeout_ms)
            .expect("reactor poll failed");

        // Every timer whose moment has passed: wake the sleepers and forget it.
        {
            let mut wake_times = shared.wake_times.lock().unwrap();
            while let Some(entry) = wake_times.first_entry()
                && *entry.key() <= Instant::now()
            {
                entry.remove().into_iter().for_each(Waker::wake);
            }
        }
    }
}
