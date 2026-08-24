//! Tasks: one piece of running work, and the queue they wait in.
//!
//! A [`Task`] is a future plus the one thing it needs to be re-run later: a
//! way back onto the ready queue. That is the whole trick behind async in
//! Rust. When a future can't finish yet it hands out a **waker**, and here the
//! waker is just "push this task onto the queue again". Whoever the future was
//! waiting on (the reactor, a channel) calls it when the wait is over, and a
//! worker thread picks the task up on its next lap.
//!
//! The other half of the file is [`JoinHandle`] - the receipt you get from
//! [`spawn`](crate::spawn), and the shared slot the task's result lands in.

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

/// Whatever value was passed to `panic!`, boxed up so it can be moved between
/// threads and re-raised later.
pub(crate) type Panic = Box<dyn Any + Send + 'static>;

/// Run `future` to completion, turning a panic into an `Err` instead of
/// letting it tear the thread down.
///
/// A panic inside a task should reach the person who awaited it, not kill the
/// worker that happened to be running it. This catches it at the boundary so
/// it can be carried somewhere safer and thrown again there.
///
/// Once the future panics it is never polled again - it is dropped when the
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

/// The list of tasks that are ready to run, shared by every worker thread.
///
/// First in, first out. The `Condvar` is what lets a worker with nothing to do
/// sleep instead of spinning: it parks until someone pushes.
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

    /// Add a task to the back of the queue and wake one waiting worker.
    pub(crate) fn push(&self, task: Arc<Task>) {
        self.tasks.lock().unwrap().push_back(task);
        self.ready.notify_one();
    }

    /// Take the next task, waiting here for as long as the queue is empty.
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

/// Any future, with its type erased, so tasks of different shapes can sit in
/// the same queue. Boxed and pinned because a future must not move once it has
/// started running.
pub type DynFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub(crate) type TaskQueue = Arc<Queue>;

/// One spawned unit of work.
///
/// Holds the future to run and a way back to the queue it belongs to - which
/// is everything needed to reschedule itself when it gets woken.
pub(crate) struct Task {
    /// `None` once the future has completed or panicked, so a stray wake can
    /// never poll it again.
    pub(crate) future: Mutex<Option<DynFuture>>,
    pub(crate) queue: TaskQueue,
}

impl Task {
    /// Put this task in line to be polled.
    fn schedule(self: &Arc<Self>) {
        self.queue.push(self.clone())
    }

    /// Wrap `future` in a task and queue it up.
    pub(crate) fn spawn(future: DynFuture, queue: &TaskQueue) {
        let task = Arc::new(Task {
            future: Mutex::new(Some(future)),
            queue: queue.clone(),
        });

        task.schedule();
    }
    /// Give the future one chance to make progress.
    ///
    /// It either finishes or returns `Pending`, in which case it has kept a
    /// clone of this task's waker and someone else will schedule us again.
    /// Doing nothing when the slot is empty is what makes a duplicate wake
    /// harmless.
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

/// "Waking" a task simply means putting it back on the ready queue.
///
/// This impl is where a `Waker` comes from: `Waker::from(arc_task)` in
/// [`Task::poll`] hands the future a callback, and this is that callback.
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

/// The shared slot between a task and whoever is waiting on its result.
///
/// It moves in one direction only: `Unawaited` -> maybe `Awaited` -> `Ready`
/// -> `Done`. Both sides race to touch it, so which one arrives first decides
/// the path:
///
/// - the task finishes first, and the result sits in `Ready` until collected;
/// - or the awaiter arrives first, leaves its waker in `Awaited`, and the task
///   uses that waker to say "your value is here".
pub(crate) enum JoinState<T> {
    /// Still running, nobody is waiting yet.
    Unawaited,
    /// Still running, and someone is parked on it - wake them when done.
    Awaited(Waker),
    /// Finished. Holds the value, or the panic it died with.
    Ready(Result<T, Panic>),
    /// The result has been taken. Polling again is a bug.
    Done,
}

/// What [`spawn`](crate::spawn) gives you: a receipt for a running task.
///
/// `.await` it to get the task's return value. The task runs whether or not
/// you ever do.
pub struct JoinHandle<T> {
    pub(crate) state: Arc<Mutex<JoinState<T>>>,
}

/// Awaiting the handle either takes the finished value or leaves a waker
/// behind and tries again once the task signals it.
///
/// # Panics
///
/// If the task panicked, that same panic is re-raised here, on the awaiting
/// task's thread.
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

/// Wrap a spawned future so its result ends up in the join slot.
///
/// Tasks in the queue all have to look the same (`Future<Output = ()>`), so
/// this is the adapter: run the real future, catch a panic if there is one,
/// drop the outcome into the shared state, and wake anyone waiting on it.
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
