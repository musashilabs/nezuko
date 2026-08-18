use nezuko::{Runtime, sleep, spawn};
use std::time::{Duration, Instant};

#[test]
fn two_tasks_sleep_concurrently() {
    let rt = Runtime::new().unwrap();
    let start = Instant::now();

    rt.block_on(async {
        let h1 = spawn(async { sleep(Duration::from_secs(1)).await });
        let h2 = spawn(async { sleep(Duration::from_secs(1)).await });
        h1.await;
        h2.await;
    });

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(1500),
        "tasks didn't run concurrently"
    );
}
