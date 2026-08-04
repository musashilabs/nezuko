use criterion::{Criterion, criterion_group, criterion_main};

fn spawn_noop(c: &mut Criterion) {
    c.bench_function("runtime/construct", |b| {
        b.iter(|| {
            // let _ = nezuko::Runtime::new();
        });
    });
}

criterion_group!(benches, spawn_noop);
criterion_main!(benches);
