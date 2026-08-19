#![allow(dead_code)]

mod shared;
use crate::{
    error,
    task::{DynFuture, JoinHandle, JoinState, wrap_with_join_state},
};
use shared::Shared;
use std::cell::RefCell;
use std::os::fd::RawFd;
use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::Instant,
};

thread_local! {
    static CURRENT: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) }
}

pub(crate) fn register_sleep(wake_time: Instant, waker: Waker) {
    CURRENT.with(|c| {
        let borrow = c.borrow();
        let shared = borrow
            .as_ref()
            .expect("sleep called outside of a nezuko runtime");

        shared
            .wake_times
            .lock()
            .unwrap()
            .entry(wake_time)
            .or_default()
            .push(waker);
    });
}

pub(crate) fn register_io(fd: RawFd, events: libc::c_short, waker: Waker) {
    CURRENT.with(|c| {
        let borrow = c.borrow();

        let shared = borrow
            .as_ref()
            .expect("I/O attempt outside of a nezuko runtime");

        shared.reactor.lock().unwrap().register(fd, events, waker);
    });
}

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let join_state = Arc::new(Mutex::new(JoinState::Unawaited));

    let join_handle = JoinHandle {
        state: Arc::clone(&join_state),
    };

    CURRENT.with(|c| {
        let shared = c.borrow().as_ref().expect("spawn outside runtime").clone();
        // wrap_with_join_state, push to shared.new_tasks, return handle
        let task = wrap_with_join_state(future, join_state);
        shared.new_tasks.lock().unwrap().push(Box::pin(task));
    });

    join_handle
}
struct CurrentGuard;

impl Drop for CurrentGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            *c.borrow_mut() = None;
        });
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

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        // register this runtime as current for this thread
        CURRENT.with(|c| {
            *c.borrow_mut() = Some(Arc::clone(&self.shared));
        });

        let _guard = CurrentGuard;

        let waker = Waker::from(self.shared.awake_flag.clone());
        let mut cx = Context::from_waker(&waker);
        let mut main_task = Box::pin(future);
        let mut other_tasks: Vec<DynFuture> = Vec::new();

        loop {
            self.shared.awake_flag.clear();

            // whole runtime is done
            if let Poll::Ready(output) = main_task.as_mut().poll(&mut cx) {
                return output; // guard clear hojega
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
