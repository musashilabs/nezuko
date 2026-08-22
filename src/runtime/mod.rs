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

pub struct Runtime {
    handle: Handle,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        let shared = Arc::new(Shared::new()?);
        Ok(Runtime {
            handle: Handle::new(shared),
        })
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let _guard = self.handle.enter();

        let num_workers = 3; // ya std::thread::available_parallelism()
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

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Handle::current().spawn(future)
}

pub fn run_reactor(shared: Arc<Shared>) {
    loop {
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
