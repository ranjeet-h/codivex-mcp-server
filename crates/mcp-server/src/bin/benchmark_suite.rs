use std::{fs, path::PathBuf, time::Instant};

use common::{
    CodeChunk,
    projects::{self, IndexedChunk, IndexedProject},
};
use embeddings::{EmbeddingConfig, EmbeddingEngine};
use indexer::{extract_chunks_for_file, incremental::ByteEdit, incremental::incremental_reparse};
use mcp_server::services::search::scoped_project_results;
use search_core::lexical::TantivyLexicalIndex;
use serde::Serialize;
use tree_sitter::Point;

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    dataset_profile: String,
    dataset_path: String,
    files_scanned: usize,
    chunks_extracted: usize,
    cold_start_index_ms: u128,
    incremental_update_ms: u128,
    query_latency_ms: u128,
    full_hybrid_query_latency_ms: u128,
    throughput_qps_estimate: f64,
    query_embedding_ms: u128,
}

struct PreparedDataset {
    cwd: PathBuf,
    project_path: String,
    files_scanned: usize,
    chunks_extracted: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dataset_profile =
        std::env::var("BENCHMARK_DATASET_PROFILE").unwrap_or_else(|_| "custom".to_string());
    let dataset_path = std::env::var("BENCHMARK_DATASET_PATH").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .display()
            .to_string()
    });
    let query = std::env::var("BENCHMARK_QUERY").unwrap_or_else(|_| "iso to date".to_string());
    let max_files = std::env::var("BENCHMARK_MAX_FILES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200);

    let prepared = prepare_dataset(&dataset_path, max_files)?;
    let cold = bench_cold_start_indexing()?;
    let incr = bench_incremental_update()?;
    let query_latency = bench_query_latency(&prepared, &query)?;
    let full_hybrid = bench_full_hybrid_query_latency(&prepared, &query).await?;
    let qps = bench_throughput_estimate(&prepared, &query).await?;
    let embed = bench_query_embedding();

    let report = BenchmarkReport {
        dataset_profile,
        dataset_path: display_dataset_path(&dataset_path),
        files_scanned: prepared.files_scanned,
        chunks_extracted: prepared.chunks_extracted,
        cold_start_index_ms: cold,
        incremental_update_ms: incr,
        query_latency_ms: query_latency,
        full_hybrid_query_latency_ms: full_hybrid,
        throughput_qps_estimate: qps,
        query_embedding_ms: embed,
    };

    let json = serde_json::to_string_pretty(&report)?;
    println!("{json}");

    let out_dir = PathBuf::from("benchmarks");
    fs::create_dir_all(&out_dir)?;
    fs::write(out_dir.join("latest-report.json"), &json)?;
    Ok(())
}

fn display_dataset_path(dataset_path: &str) -> String {
    let redact = std::env::var("BENCHMARK_REDACT_PATH")
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !redact {
        return dataset_path.to_string();
    }
    std::path::Path::new(dataset_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|name| format!("<redacted:{name}>"))
        .unwrap_or_else(|| "<redacted>".to_string())
}

fn prepare_dataset(dataset_path: &str, max_files: usize) -> anyhow::Result<PreparedDataset> {
    let bench_root = std::env::temp_dir().join(format!(
        "codivex-benchmark-{}-{}",
        std::process::id(),
        unix_now()
    ));
    std::fs::create_dir_all(&bench_root)?;

    let project_root = PathBuf::from(dataset_path);
    let mut files = indexer::scanner::scan_source_files(&project_root);
    files.truncate(max_files);

    let mut chunks = Vec::<CodeChunk>::new();
    for file in &files {
        if let Ok(content) = std::fs::read_to_string(file) {
            if let Ok(extracted) =
                extract_chunks_for_file(file.to_string_lossy().as_ref(), &content)
            {
                chunks.extend(extracted);
            }
        }
    }

    let indexed_chunks = chunks
        .iter()
        .map(|c| IndexedChunk {
            file: c.file_path.clone(),
            symbol: c.symbol.clone(),
            start_line: c.start_line,
            end_line: c.end_line,
            content: c.content.clone(),
        })
        .collect::<Vec<_>>();

    let indexed = IndexedProject {
        project_path: project_root.display().to_string(),
        files_scanned: files.len(),
        chunks_extracted: chunks.len(),
        indexed_at_unix: unix_now(),
        chunks: indexed_chunks,
    };
    projects::save_project_index(&bench_root, &indexed)?;

    let lexical_dir = projects::project_lexical_index_dir(&bench_root, &indexed.project_path);
    let mut lexical = TantivyLexicalIndex::open_or_create_on_disk(&lexical_dir)?;
    lexical.reset()?;
    for chunk in &chunks {
        lexical.add_chunk(chunk)?;
    }
    lexical.commit()?;

    Ok(PreparedDataset {
        cwd: bench_root,
        project_path: indexed.project_path,
        files_scanned: files.len(),
        chunks_extracted: chunks.len(),
    })
}

fn bench_cold_start_indexing() -> anyhow::Result<u128> {
    let start = Instant::now();
    let content = "fn iso_to_date(input: &str) -> String { input.to_string() }";
    let _chunks = extract_chunks_for_file("src/date.rs", content)?;
    Ok(start.elapsed().as_millis())
}

fn bench_incremental_update() -> anyhow::Result<u128> {
    let start = Instant::now();
    let old_source = "fn a() { 1 }\n";
    let new_source = "fn a() { 2 }\n";
    let edit = ByteEdit {
        start_byte: 9,
        old_end_byte: 10,
        new_end_byte: 10,
        start_position: Point { row: 0, column: 9 },
        old_end_position: Point { row: 0, column: 10 },
        new_end_position: Point { row: 0, column: 10 },
    };
    let _ = incremental_reparse("src/lib.rs", old_source, new_source, edit)?;
    Ok(start.elapsed().as_millis())
}

fn bench_query_latency(prepared: &PreparedDataset, query: &str) -> anyhow::Result<u128> {
    let lexical_dir = projects::project_lexical_index_dir(&prepared.cwd, &prepared.project_path);
    let lexical = TantivyLexicalIndex::open_or_create_on_disk(&lexical_dir)?;
    let start = Instant::now();
    let _ = lexical.search_ids(query, 5)?;
    Ok(start.elapsed().as_millis())
}

async fn bench_full_hybrid_query_latency(
    prepared: &PreparedDataset,
    query: &str,
) -> anyhow::Result<u128> {
    let start = Instant::now();
    let _ = scoped_project_results(&prepared.cwd, &prepared.project_path, query, 5).await?;
    Ok(start.elapsed().as_millis())
}

async fn bench_throughput_estimate(prepared: &PreparedDataset, query: &str) -> anyhow::Result<f64> {
    let ops = 250usize;
    let start = Instant::now();
    for _ in 0..ops {
        let _ = scoped_project_results(&prepared.cwd, &prepared.project_path, query, 5).await?;
    }
    let secs = start.elapsed().as_secs_f64();
    if secs == 0.0 {
        return Ok(ops as f64);
    }
    Ok(ops as f64 / secs)
}

fn bench_query_embedding() -> u128 {
    let engine = EmbeddingEngine::new(EmbeddingConfig::default());
    let start = Instant::now();
    let _ = engine.embed_batch(&["save user record".to_string()]).ok();
    start.elapsed().as_millis()
}

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}
