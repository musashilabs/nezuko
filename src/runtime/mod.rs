#![allow(dead_code)]

mod handle;
mod shared;

use crate::{
    error,
    task::{DynFuture, JoinHandle},
};
pub use handle::Handle;
use shared::Shared;
use std::{
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Instant,
};

pub struct Runtime {
    handle: Handle,
}

impl Runtime {
    pub fn new() -> error::Result<Self> {
        let shared = Arc::new(Shared::new());
        Ok(Runtime {
            handle: Handle::new(shared),
        })
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _guard = self.handle.enter();
        let shared = self.handle.shared();

        let waker = Waker::from(shared.awake_flag.clone());
        let mut cx = Context::from_waker(&waker);
        let mut main_task = Box::pin(future);
        let mut other_tasks: Vec<DynFuture> = Vec::new();

        loop {
            shared.awake_flag.clear();

            // whole runtime is done
            if let Poll::Ready(output) = main_task.as_mut().poll(&mut cx) {
                return output; // guard clear hojega
            }

            other_tasks.retain_mut(|task| task.as_mut().poll(&mut cx).is_pending());

            loop {
                let Some(mut task) = shared.new_tasks.lock().unwrap().pop() else {
                    break;
                };
                if task.as_mut().poll(&mut cx).is_pending() {
                    other_tasks.push(task);
                }
            }

            if shared.awake_flag.is_set() {
                continue;
            }

            let timeout_ms = {
                let wake_times = shared.wake_times.lock().unwrap();
                if let Some(&next) = wake_times.keys().next() {
                    let dur = next.saturating_duration_since(Instant::now());
                    dur.as_millis() as libc::c_int
                } else {
                    -1 // no timer .. block forever
                }
            };

            shared
                .reactor
                .lock()
                .unwrap()
                .poll_and_wake(timeout_ms)
                .expect("reactor poll failed");

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
}

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Handle::current().spawn(future)
}
