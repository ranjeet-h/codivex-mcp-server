use criterion::{Criterion, criterion_group, criterion_main};
use embeddings::{EmbeddingConfig, EmbeddingEngine};

fn bench_embedding(c: &mut Criterion) {
    let engine = EmbeddingEngine::new(EmbeddingConfig::default());
    let batch = (0..128)
        .map(|i| format!("sample text {i}"))
        .collect::<Vec<_>>();

    c.bench_function("embedding_batch_128", |b| {
        b.iter(|| {
            let _ = engine.embed_batch(&batch).ok();
        })
    });
}

criterion_group!(benches, bench_embedding);
criterion_main!(benches);
