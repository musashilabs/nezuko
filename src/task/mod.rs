use std::{pin::Pin, sync::{Arc, Mutex}, task::{Poll, Wake, Waker}};

pub type DynFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub struct AwakeFlag(Mutex<bool>);

impl Wake for AwakeFlag {
    fn wake(self: Arc<Self>) {
        *self.0.lock().unwrap() = true;
    }
}

pub enum JoinState<T> {
    Unawaited,
    Awaited(Waker),
    Ready(T),
    Done,
}

pub struct JoinHandle<T> {
    pub state: Arc<Mutex<JoinState<T>>>,
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

pub async fn wrap_with_join_state<F: Future>(future: F, join_state: Arc<Mutex<JoinState<F::Output>>>) {
    let value = future.await; // 1. asli future chalao
    let mut guard = join_state.lock().unwrap();
    if let JoinState::Awaited(waker) = &*guard {
       
        waker.wake_by_ref();
    }
    *guard = JoinState::Ready(value); 
}