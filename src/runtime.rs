#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    task::Waker,
    time::Instant,
};

use crate::{
    error,
    task::{DynFuture, JoinHandle, JoinState, wrap_with_join_state},
};

struct Shared {
    new_tasks: Mutex<Vec<DynFuture>>,
    wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            new_tasks: Mutex::new(Vec::new()),
            wake_times: Mutex::new(BTreeMap::new()),
        }
    }
}

pub struct Runtime {
    shared: Arc<Shared>,
}

impl Runtime {
    pub fn new() -> error::Result<Self> {
        Ok(Self {
            shared: Arc::new(Shared::new()),
        })
    }

    pub fn spawn<F, T>(&self, future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send,
    {
        let join_state = Arc::new(Mutex::new(JoinState::Unawaited));

        let join_handle = JoinHandle {
            state: Arc::clone(&join_state),
        };

        self.shared
            .new_tasks
            .lock()
            .unwrap()
            .push(Box::pin(wrap_with_join_state(future, join_state)));

        join_handle
    }

    pub fn block_on<F: Future>(&self, _fut: F) -> F::Output {
        todo!("wire up the executor")
    }
}
