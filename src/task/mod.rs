use std::any::Any;
use std::collections::VecDeque;
use std::future::poll_fn;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Condvar;
use std::task::Context;
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Poll, Wake, Waker},
};

/// The payload carried by an unwinding panic, as produced by `catch_unwind`.
pub(crate) type Panic = Box<dyn Any + Send + 'static>;

/// Run `future` to completion, capturing a panic instead of unwinding.
///
/// Once the future panics it is never polled again.. it is dropped when the
/// returned future is.
pub(crate) async fn catch_unwind<F: Future>(future: F) -> Result<F::Output, Panic> {
    // Boxed so the closure below can hold it across polls without unsafe pin
    let mut future = Box::pin(future);

    poll_fn(
        move |cx| match panic::catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(payload) => Poll::Ready(Err(payload)),
        },
    )
    .await
}

pub(crate) struct Queue {
    tasks: Mutex<VecDeque<Arc<Task>>>,
    ready: Condvar,
}
impl Queue {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
        }
    }
    pub(crate) fn push(&self, task: Arc<Task>) {
        self.tasks.lock().unwrap().push_back(task);
        self.ready.notify_one();
    }
    pub(crate) fn pop(&self) -> Arc<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        loop {
            if let Some(task) = tasks.pop_front() {
                return task;
            }
            tasks = self.ready.wait(tasks).unwrap();
        }
    }
}

pub type DynFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub(crate) type TaskQueue = Arc<Queue>;

pub(crate) struct Task {
    /// `None` once the future has completed or panicked, so a stray wake can
    /// never poll it again.
    pub(crate) future: Mutex<Option<DynFuture>>,
    pub(crate) queue: TaskQueue,
}

impl Task {
    fn schedule(self: &Arc<Self>) {
        self.queue.push(self.clone())
    }

    pub(crate) fn spawn(future: DynFuture, queue: &TaskQueue) {
        let task = Arc::new(Task {
            future: Mutex::new(Some(future)),
            queue: queue.clone(),
        });

        task.schedule();
    }
    pub(crate) fn poll(self: &Arc<Self>) {
        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);
        let mut slot = self.future.lock().unwrap();

        let result = {
            let Some(future) = slot.as_mut() else { return };
            // a panic here must not take down the worker thread.
            // Catching inside the guard's scope also leaves the mutex
            // unpoisoned, so the task stays inspectable.
            panic::catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(&mut cx)))
        };

        // Completed or panicked: drop the future now rather than waiting for
        // the last Arc to go away.
        if !matches!(result, Ok(Poll::Pending)) {
            *slot = None;
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.schedule();
    }
}
//
// pub struct AwakeFlag(Mutex<bool>);
//
// impl AwakeFlag {
//     pub fn new() -> Self {
//         Self(Mutex::new(false))
//     }
//
//     pub fn clear(&self) {
//         *self.0.lock().unwrap() = false;
//     }
//
//     pub fn is_set(&self) -> bool {
//         *self.0.lock().unwrap()
//     }
// }
//
// impl Default for AwakeFlag {
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
// impl Wake for AwakeFlag {
//     fn wake(self: Arc<Self>) {
//         *self.0.lock().unwrap() = true;
//     }
// }

pub(crate) enum JoinState<T> {
    Unawaited,
    Awaited(Waker),
    Ready(Result<T, Panic>),
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
            JoinState::Ready(Ok(value)) => Poll::Ready(value),
            JoinState::Ready(Err(payload)) => {
                // Release the lock before unwinding, otherwise the panic
                // poisons the join state on its way out.
                drop(guard);
                panic::resume_unwind(payload);
            }
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
    let result = catch_unwind(future).await;
    let mut guard = join_state.lock().unwrap();
    if let JoinState::Awaited(waker) = &*guard {
        waker.wake_by_ref();
    }
    *guard = JoinState::Ready(result);
}
