//! A channel that carries exactly one value, once.
//!
//! [`channel`] hands back a matched pair: a [`Sender`] that can be used a
//! single time, and a [`Receiver`] that yields that one value. They share one
//! slot behind an `Arc`, and `send` consumes the sender, so "exactly once" is
//! enforced by the types rather than by care.
//!
//! What makes it useful here is that the receiving end works both ways: you can
//! `.await` it from inside a task, or call
//! [`recv_blocking`](Receiver::recv_blocking) to park an ordinary thread on it.
//! That is exactly the handoff [`block_on`](crate::Runtime::block_on) needs -
//! a worker thread finishes the future and sends, while the thread that called
//! `block_on` is blocked on the other end.

use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

/// The slot itself: the value once it arrives, and the waker of an async
/// receiver that got there first.
struct Inner<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

/// Shared between the two ends. The `Condvar` serves blocking receivers, the
/// `Waker` inside `Inner` serves async ones - both are kept so either style
/// works.
struct Channel<T> {
    inner: Mutex<Inner<T>>,
    ready: Condvar,
}

/// The sending half. Consumed by [`send`](Sender::send), so it fires once.
pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    /// Deliver the value and wake the receiver, whichever way it is waiting.
    ///
    /// The waker fires while the lock is held, but `notify_one` comes after
    /// `drop(inner)` so a blocking receiver doesn't wake up onto a lock that is
    /// still taken.
    pub fn send(self, value: T) {
        let mut inner = self.channel.inner.lock().unwrap();
        inner.value = Some(value);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
        drop(inner);
        self.channel.ready.notify_one();
    }
}
/// The receiving half. Either `.await` it, or call
/// [`recv_blocking`](Self::recv_blocking).
pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Receiver<T> {
    /// Park this whole thread until the value arrives.
    ///
    /// For plain threads that are not running async code - this is how
    /// [`block_on`](crate::Runtime::block_on) waits.
    pub fn recv_blocking(self) -> T {
        let mut inner = self.channel.inner.lock().unwrap();
        loop {
            match inner.value.take() {
                Some(val) => return val,
                None => inner = self.channel.ready.wait(inner).unwrap(),
            }
        }
    }
}
/// The async way to wait: take the value if it's there, otherwise leave a
/// waker for the sender to call.
impl<T> Future for Receiver<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<T> {
        let mut inner = self.channel.inner.lock().unwrap();
        match inner.value.take() {
            Some(val) => Poll::Ready(val),
            None => {
                inner.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Create a connected sender/receiver pair.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel {
        inner: Mutex::new(Inner {
            value: None,
            waker: None,
        }),
        ready: Condvar::new(),
    });
    (
        Sender {
            channel: channel.clone(),
        },
        Receiver { channel },
    )
}
