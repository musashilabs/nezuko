// ek channel do end me value share krega to Arc se koi value

use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

struct Inner<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

struct Channel<T> {
    inner: Mutex<Inner<T>>,
    ready: Condvar,
}

pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
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
pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Receiver<T> {
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
