use crate::reactor::Reactor;
use crate::task::{Queue, TaskQueue};
use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Instant;

/// Everything the runtime's threads need to see, in one place.
///
/// Kept behind an `Arc` so the workers, the reactor thread, and every
/// [`Handle`](super::Handle) all point at the same instance.
pub(crate) struct Shared {
    /// Tasks that are ready to run right now. Workers pop from here.
    pub(crate) queue: TaskQueue,
    /// Who to wake, and when. A `BTreeMap` because it keeps the keys sorted,
    /// so the reactor can read the soonest deadline off the front.
    pub(crate) wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
    /// Watches sockets and owns the pipe used to interrupt the wait.
    pub(crate) reactor: Reactor,
}

impl Shared {
    /// Build the shared state. Fails only if the reactor's wakeup pipe can't
    /// be opened.
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            queue: Arc::new(Queue::new()),
            wake_times: Mutex::new(BTreeMap::new()),
            reactor: Reactor::new()?,
        })
    }
}
