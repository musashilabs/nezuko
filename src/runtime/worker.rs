use crate::task::TaskQueue;

pub fn run_worker(queue: TaskQueue) {
    loop {
        let task = queue.pop();
        task.poll()
    }
}
