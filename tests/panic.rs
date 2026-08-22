use nezuko::{Runtime, sleep, spawn};
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

/// A panicking task must not take a worker thread down with it.. the runtime
/// keeps making progress and block_on still returns.
#[test]
fn panicking_task_does_not_kill_runtime() {
    let rt = Runtime::new().unwrap();

    let value = rt.block_on(async {
        // More panicking tasks than there are workers.. if a panic killed its
        // worker, the pool would be drained and nothing below would run.
        for i in 0..8u32 {
            spawn(async move { panic!("task {i} blew up") });
        }

        // Enough work to reuse every worker a panicking task may have run on.
        let mut handles = Vec::new();
        for i in 0..16u32 {
            handles.push(spawn(async move {
                sleep(Duration::from_millis(10)).await;
                i
            }));
        }

        let mut total = 0;
        for handle in handles {
            total += handle.await;
        }
        total
    });

    assert_eq!(value, (0..16).sum::<u32>());
}

/// Awaiting a JoinHandle for a task that panicked re-raises that panic in the
/// awaiting task, rather than hanging forever.
#[test]
fn join_handle_propagates_panic() {
    let rt = Runtime::new().unwrap();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(async {
            let handle = spawn(async {
                sleep(Duration::from_millis(10)).await;
                panic!("inner panic");
            });
            handle.await
        })
    }));

    let payload = result.expect_err("panic should have propagated");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"inner panic"));
}

/// A panic in the top-level future surfaces on the `block_on` caller's thread.
#[test]
fn block_on_propagates_panic() {
    let rt = Runtime::new().unwrap();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(async {
            sleep(Duration::from_millis(10)).await;
            panic!("top level panic");
        })
    }));

    let payload = result.expect_err("panic should have propagated");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"top level panic"));
}
