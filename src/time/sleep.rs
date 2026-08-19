use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use crate::runtime::Handle;
pub struct Sleep {
    wake_time: Instant,
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        wake_time: Instant::now() + duration,
    }
}

impl Future for Sleep {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<()> {
        if Instant::now() >= self.wake_time {
            Poll::Ready(())
        } else {
            Handle::current().register_sleep(self.wake_time, context.waker().clone());
            Poll::Pending
        }
    }
}
