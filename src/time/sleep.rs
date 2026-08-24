use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use crate::runtime::Handle;

/// The future returned by [`sleep`]. Does nothing until awaited.
///
/// It stores the moment to wake up rather than the duration, so being polled
/// early or late doesn't stretch the wait.
pub struct Sleep {
    wake_time: Instant,
}

/// Pause the current task for `duration`, letting other tasks run meanwhile.
///
/// This is the async cousin of `std::thread::sleep`. That one puts a whole
/// thread to sleep; this one only parks the task, so the worker thread moves
/// straight on to something else.
///
/// ```
/// use nezuko::{Runtime, sleep};
/// use std::time::{Duration, Instant};
///
/// let rt = Runtime::new().unwrap();
///
/// rt.block_on(async {
///     let start = Instant::now();
///     sleep(Duration::from_millis(50)).await;
///     assert!(start.elapsed() >= Duration::from_millis(50));
/// });
/// ```
///
/// Nothing happens until you `.await` it - `sleep(d);` on its own is a no-op.
/// Timing is best-effort: you get *at least* `duration`, maybe a little more.
///
/// # Panics
///
/// Panics when awaited outside a running runtime, since there is no reactor to
/// register the timer with.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        wake_time: Instant::now() + duration,
    }
}

/// Either the moment has arrived, or we leave our waker in the runtime's timer
/// list and let the reactor call it when the clock catches up.
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
