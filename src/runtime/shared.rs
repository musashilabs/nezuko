use crate::reactor::Reactor;
use crate::task::{Queue, TaskQueue};
use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Instant;

pub(crate) struct Shared {
    pub(crate) queue: TaskQueue,
    pub(crate) wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
    pub(crate) reactor: Reactor,
}

impl Shared {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            queue: Arc::new(Queue::new()),
            wake_times: Mutex::new(BTreeMap::new()),
            reactor: Reactor::new()?,
        })
    }
}
