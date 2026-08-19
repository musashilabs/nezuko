use crate::task::{AwakeFlag, DynFuture};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Instant;

pub(crate) struct Shared {
    pub new_tasks: Mutex<Vec<DynFuture>>,
    pub wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
    pub awake_flag: Arc<AwakeFlag>,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            new_tasks: Mutex::new(Vec::new()),
            wake_times: Mutex::new(BTreeMap::new()),
            awake_flag: Arc::new(AwakeFlag::default()),
        }
    }
}
