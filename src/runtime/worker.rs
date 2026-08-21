use crate::runtime::Handle;
use crate::task::TaskQueue;

pub fn run_worker(handle: Handle, queue: TaskQueue) {
    let _guard = handle.enter();
    loop {
        let task = queue.pop();
        task.poll()
    }
}
