use criterion::{Criterion, criterion_group, criterion_main};
use search_core::rrf_fuse;

fn bench_fusion(c: &mut Criterion) {
    let lexical = (0..100).map(|i| format!("l{i}")).collect::<Vec<_>>();
    let vector = (0..100).rev().map(|i| format!("l{i}")).collect::<Vec<_>>();

    c.bench_function("rrf_fuse_100", |b| {
        b.iter(|| {
            let _ = rrf_fuse(&lexical, &vector, 60, 1.0, 0.7);
        })
    });
}

criterion_group!(benches, bench_fusion);
criterion_main!(benches);
