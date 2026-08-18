#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::Instant,
};

use crate::{
    error,
    task::{AwakeFlag, DynFuture, JoinHandle, JoinState, wrap_with_join_state},
};

struct Shared {
    new_tasks: Mutex<Vec<DynFuture>>,
    wake_times: Mutex<BTreeMap<Instant, Vec<Waker>>>,
    awake_flag: Arc<AwakeFlag>,
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

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let waker = Waker::from(self.shared.awake_flag.clone());
        let mut cx = Context::from_waker(&waker);
        let mut main_task = Box::pin(future);
        let mut other_tasks: Vec<DynFuture> = Vec::new();

        loop {
            self.shared.awake_flag.clear();

            // whole runtime is done
            if let Poll::Ready(output) = main_task.as_mut().poll(&mut cx) {
                return output;
            }

            other_tasks.retain_mut(|task| task.as_mut().poll(&mut cx).is_pending());

            loop {
                let Some(mut task) = self.shared.new_tasks.lock().unwrap().pop() else {
                    break;
                };
                if task.as_mut().poll(&mut cx).is_pending() {
                    other_tasks.push(task);
                }
            }

            if self.shared.awake_flag.is_set() {
                continue;
            }

            let mut wake_times = self.shared.wake_times.lock().unwrap();
            let next_wake = wake_times.keys().next().expect("kuch to gadbad h dya");
            std::thread::sleep(next_wake.saturating_duration_since(Instant::now()));

            while let Some(entry) = wake_times.first_entry()
                && *entry.key() <= Instant::now()
            {
                entry.remove().into_iter().for_each(Waker::wake);
            }
        }
    }
}
