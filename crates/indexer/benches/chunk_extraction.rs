use criterion::{Criterion, criterion_group, criterion_main};
use indexer::extract_chunks_for_file;

fn bench_chunk_extraction(c: &mut Criterion) {
    let content = r#"
    /// docs
    fn iso_to_date(input: &str) -> String { input.to_string() }
    fn parse_date(input: &str) -> String { input.to_string() }
    "#;

    c.bench_function("chunk_extraction_rust", |b| {
        b.iter(|| {
            let _ = extract_chunks_for_file("src/date.rs", content).expect("extract");
        })
    });
}

criterion_group!(benches, bench_chunk_extraction);
criterion_main!(benches);
