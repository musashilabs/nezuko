use crate::runtime::Handle;
use crate::task::TaskQueue;

/// A worker thread: take the next ready task, poll it, repeat forever.
///
/// This is the whole executor. Several of these run at once, all pulling from
/// the same queue, which is what makes tasks run in parallel. `queue.pop()`
/// blocks while the queue is empty, so an idle worker costs nothing.
///
/// Entering the handle first means code inside a task can call
/// [`spawn`](crate::spawn) and [`sleep`](crate::sleep) and still find its
/// runtime.
pub fn run_worker(handle: Handle, queue: TaskQueue) {
    let _guard = handle.enter();
    loop {
        let task = queue.pop();
        task.poll()
    }
}
