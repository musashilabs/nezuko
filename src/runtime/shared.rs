use crate::reactor::Reactor;
use crate::task::{AwakeFlag, DynFuture};
use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Instant;

pub(crate) struct Shared {
    pub(crate) new_tasks: Mutex<Vec<DynFuture>>,
    pub(crate) wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
    pub(crate) awake_flag: Arc<AwakeFlag>,
    pub(crate) reactor: Mutex<Reactor>,
}

impl Shared {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            new_tasks: Mutex::new(Vec::new()),
            wake_times: Mutex::new(BTreeMap::new()),
            awake_flag: Arc::new(AwakeFlag::default()),
            reactor: Mutex::new(Reactor::new()?),
        })
    }
}
