//! "Which runtime am I inside of?"
//!
//! Free functions like [`spawn`](crate::spawn) and [`sleep`](crate::sleep) are
//! not passed a runtime, so they have to look it up. Every thread that runs
//! runtime code stores a [`Handle`] in a thread-local, and those functions read
//! it back out via [`Handle::current`].

use std::{
    cell::RefCell,
    future::Future,
    os::fd::RawFd,
    sync::{Arc, Mutex},
    task::Waker,
    time::Instant,
};

use crate::runtime::Shared;
use crate::task::{JoinHandle, JoinState, Task, wrap_with_join_state};

thread_local! {
    /// Set by [`Handle::enter`] while a thread is running runtime code.
    static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

/// A pointer to a running runtime that is cheap to clone and hand around.
///
/// It holds nothing itself - all the real state lives in the [`Shared`] behind
/// it. Cloning a `Handle` just bumps a refcount, which is what lets every
/// worker thread hold one.
#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}

impl Handle {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Handle { shared }
    }

    /// The runtime this thread is currently running inside.
    ///
    /// # Panics
    ///
    /// Panics if this thread is not inside a runtime - almost always because
    /// something was called outside of [`block_on`](super::Runtime::block_on).
    pub fn current() -> Handle {
        CURRENT.with(|c| {
            c.borrow()
                .as_ref()
                .expect("called outside of a nezuko runtime")
                .clone()
        })
    }

    /// Nudge the reactor out of its `poll()` so it re-reads what to wait for.
    pub(crate) fn wake(&self) {
        let _ = self.shared.reactor.wakeup_trigger();
    }

    /// Mark this thread as being inside this runtime.
    ///
    /// The returned guard clears the marker again when it is dropped, so the
    /// usual shape is `let _guard = handle.enter();` at the top of a function.
    pub(crate) fn enter(&self) -> EnterGuard {
        CURRENT.with(|c| {
            *c.borrow_mut() = Some(self.clone());
        });
        EnterGuard
    }

    /// Ask to be woken at `wake_time`.
    ///
    /// Adds the waker to the shared timer list, then pokes the reactor: it may
    /// be parked on a timeout that is now too long, and needs to recalculate.
    pub(crate) fn register_sleep(&self, wake_time: Instant, waker: Waker) {
        self.shared
            .wake_times
            .lock()
            .unwrap()
            .entry(wake_time)
            .or_default()
            .push(waker);

        let _ = self.shared.reactor.wakeup_trigger();
    }

    /// Ask to be woken when `fd` is ready for `events` (readable, writable, ...).
    ///
    /// One-shot: the reactor drops the registration as soon as it fires, so a
    /// future that is still not finished has to register again on its next poll.
    pub(crate) fn register_io(&self, fd: RawFd, events: libc::c_short, waker: Waker) {
        self.shared.reactor.register(fd, events, waker);
    }

    /// The work behind [`spawn`](crate::spawn).
    ///
    /// Creates the shared slot the result will land in, wraps the future so it
    /// fills that slot when it finishes, and queues it up. The caller keeps the
    /// other end of the slot as a [`JoinHandle`].
    pub(crate) fn spawn<F, T>(&self, future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let join_state = Arc::new(Mutex::new(JoinState::Unawaited));
        let join_handle = JoinHandle {
            state: Arc::clone(&join_state),
        };
        let task = wrap_with_join_state(future, join_state);
        Task::spawn(Box::pin(task), &self.shared.queue);
        join_handle
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}

/// Clears the thread's "current runtime" marker when it goes out of scope.
pub(crate) struct EnterGuard;

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            *c.borrow_mut() = None;
        });
    }
}
