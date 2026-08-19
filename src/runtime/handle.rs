use std::{
    cell::RefCell,
    future::Future,
    os::fd::RawFd,
    sync::{Arc, Mutex},
    task::Waker,
    time::Instant,
};

use crate::runtime::Shared;
use crate::task::{JoinHandle, JoinState, wrap_with_join_state};

thread_local! {
    static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}

impl Handle {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Handle { shared }
    }

    pub fn current() -> Handle {
        CURRENT.with(|c| {
            c.borrow()
                .as_ref()
                .expect("called outside of a nezuko runtime")
                .clone()
        })
    }

    pub(crate) fn enter(&self) -> EnterGuard {
        CURRENT.with(|c| {
            *c.borrow_mut() = Some(self.clone());
        });
        EnterGuard
    }

    pub(crate) fn register_sleep(&self, wake_time: Instant, waker: Waker) {
        self.shared
            .wake_times
            .lock()
            .unwrap()
            .entry(wake_time)
            .or_default()
            .push(waker);
    }

    pub(crate) fn register_io(&self, fd: RawFd, events: libc::c_short, waker: Waker) {
        self.shared
            .reactor
            .lock()
            .unwrap()
            .register(fd, events, waker);
    }

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
        self.shared.new_tasks.lock().unwrap().push(Box::pin(task));
        join_handle
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}

pub(crate) struct EnterGuard;

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            *c.borrow_mut() = None;
        });
    }
}
