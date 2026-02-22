use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BenchmarkReport {
    full_hybrid_query_latency_ms: u64,
    query_embedding_ms: u64,
    throughput_qps_estimate: f64,
}

#[derive(Debug, Deserialize)]
struct LoadReport {
    api: LoadMetrics,
}

#[derive(Debug, Deserialize)]
struct LoadMetrics {
    failures: u64,
    latency_ms: LatencyMetrics,
}

#[derive(Debug, Deserialize)]
struct LatencyMetrics {
    p50: f64,
    p95: f64,
}

fn main() -> anyhow::Result<()> {
    let benchmark_path = env_path("BENCHMARK_REPORT", "benchmarks/latest-report.json");
    let load_path = env_path("LOAD_REPORT", "benchmarks/load-test-report.json");

    let max_hybrid_ms = env_u64("SLO_MAX_HYBRID_MS", 45);
    let max_embedding_ms = env_u64("SLO_MAX_EMBEDDING_MS", 20);
    let min_throughput_qps = env_f64("SLO_MIN_THROUGHPUT_QPS", 100.0);
    let max_api_p50_ms = env_f64("SLO_MAX_API_P50_MS", 50.0);
    let max_api_p95_ms = env_f64("SLO_MAX_API_P95_MS", 200.0);

    let benchmark: BenchmarkReport =
        serde_json::from_str(&std::fs::read_to_string(&benchmark_path).with_context(|| {
            format!(
                "failed to read benchmark report {}",
                benchmark_path.display()
            )
        })?)
        .with_context(|| format!("failed to parse {}", benchmark_path.display()))?;

    let load: LoadReport = serde_json::from_str(
        &std::fs::read_to_string(&load_path)
            .with_context(|| format!("failed to read load report {}", load_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", load_path.display()))?;

    let checks = vec![
        (
            "hybrid_latency_ms",
            benchmark.full_hybrid_query_latency_ms as f64 <= max_hybrid_ms as f64,
            format!(
                "{} <= {}",
                benchmark.full_hybrid_query_latency_ms, max_hybrid_ms
            ),
        ),
        (
            "embedding_latency_ms",
            benchmark.query_embedding_ms as f64 <= max_embedding_ms as f64,
            format!("{} <= {}", benchmark.query_embedding_ms, max_embedding_ms),
        ),
        (
            "throughput_qps",
            benchmark.throughput_qps_estimate >= min_throughput_qps,
            format!(
                "{:.2} >= {:.2}",
                benchmark.throughput_qps_estimate, min_throughput_qps
            ),
        ),
        (
            "load_api_failures",
            load.api.failures == 0,
            format!("{} == 0", load.api.failures),
        ),
        (
            "load_api_p50_ms",
            load.api.latency_ms.p50 <= max_api_p50_ms,
            format!("{:.2} <= {:.2}", load.api.latency_ms.p50, max_api_p50_ms),
        ),
        (
            "load_api_p95_ms",
            load.api.latency_ms.p95 <= max_api_p95_ms,
            format!("{:.2} <= {:.2}", load.api.latency_ms.p95, max_api_p95_ms),
        ),
    ];

    let mut failed = false;
    for (name, ok, detail) in checks {
        println!("{}: {} ({detail})", name, if ok { "PASS" } else { "FAIL" });
        if !ok {
            failed = true;
        }
    }

    if failed {
        anyhow::bail!("one or more SLO checks failed");
    }
    println!("all SLO checks passed");
    Ok(())
}

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}
