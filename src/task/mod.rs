use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Poll, Wake, Waker},
};

pub type DynFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub struct AwakeFlag(Mutex<bool>);

impl AwakeFlag {
    pub fn new() -> Self {
        Self(Mutex::new(false))
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap() = false;
    }

    pub fn is_set(&self) -> bool {
        *self.0.lock().unwrap()
    }
}

impl Default for AwakeFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl Wake for AwakeFlag {
    fn wake(self: Arc<Self>) {
        *self.0.lock().unwrap() = true;
    }
}

pub(crate) enum JoinState<T> {
    Unawaited,
    Awaited(Waker),
    Ready(T),
    Done,
}

pub struct JoinHandle<T> {
    pub(crate) state: Arc<Mutex<JoinState<T>>>,
}

impl<T> Future for JoinHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut guard = self.state.lock().unwrap();
        match std::mem::replace(&mut *guard, JoinState::Done) {
            JoinState::Ready(value) => Poll::Ready(value),
            JoinState::Unawaited | JoinState::Awaited(_) => {
                // replace prev waker ( if any was there )
                *guard = JoinState::Awaited(cx.waker().clone());
                Poll::Pending
            }
            JoinState::Done => unreachable!("Poll After Ready"),
        }
    }
}

pub(crate) async fn wrap_with_join_state<F: Future>(
    future: F,
    join_state: Arc<Mutex<JoinState<F::Output>>>,
) {
    let value = future.await;
    let mut guard = join_state.lock().unwrap();
    if let JoinState::Awaited(waker) = &*guard {
        waker.wake_by_ref();
    }
    *guard = JoinState::Ready(value);
}
