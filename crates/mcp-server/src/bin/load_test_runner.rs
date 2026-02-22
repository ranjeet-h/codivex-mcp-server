use std::{fs, path::PathBuf, sync::Arc, time::Instant};

use anyhow::Context;
use mcp_server::{app, state::AppState};
use reqwest::Client;
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};

#[derive(Debug, Clone, Copy)]
struct LoadTestConfig {
    api_requests: usize,
    api_concurrency: usize,
    sse_streams: usize,
    sse_concurrency: usize,
    timeout_secs: u64,
}

#[derive(Debug, Serialize)]
struct LoadTestReport {
    base_url: String,
    api_requests: usize,
    api_concurrency: usize,
    sse_streams: usize,
    sse_concurrency: usize,
    timeout_secs: u64,
    api: PhaseReport,
    sse: PhaseReport,
}

#[derive(Debug, Serialize)]
struct PhaseReport {
    attempts: usize,
    successes: usize,
    failures: usize,
    throughput_ops_per_sec: f64,
    latency_ms: LatencyReport,
}

#[derive(Debug, Serialize)]
struct LatencyReport {
    min: f64,
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
    avg: f64,
}

struct LocalServer {
    base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = LoadTestConfig {
        api_requests: env_usize("LOAD_TEST_API_REQUESTS", 300),
        api_concurrency: env_usize("LOAD_TEST_API_CONCURRENCY", 32),
        sse_streams: env_usize("LOAD_TEST_SSE_STREAMS", 64),
        sse_concurrency: env_usize("LOAD_TEST_SSE_CONCURRENCY", 8),
        timeout_secs: env_u64("LOAD_TEST_TIMEOUT_SECS", 20),
    };

    let (base_url, _local_server) = if let Ok(url) = std::env::var("LOAD_TEST_BASE_URL") {
        (url, None)
    } else {
        let local_server = spawn_local_server().await?;
        (local_server.base_url.clone(), Some(local_server))
    };

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
        .build()?;

    let api = run_api_phase(&client, &base_url, cfg).await?;
    let sse = run_sse_phase(&client, &base_url, cfg).await?;

    let report = LoadTestReport {
        base_url,
        api_requests: cfg.api_requests,
        api_concurrency: cfg.api_concurrency,
        sse_streams: cfg.sse_streams,
        sse_concurrency: cfg.sse_concurrency,
        timeout_secs: cfg.timeout_secs,
        api,
        sse,
    };

    let json = serde_json::to_string_pretty(&report)?;
    println!("{json}");

    let out_dir = PathBuf::from("benchmarks");
    fs::create_dir_all(&out_dir)?;
    fs::write(out_dir.join("load-test-report.json"), &json)?;
    Ok(())
}

async fn spawn_local_server() -> anyhow::Result<LocalServer> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let mut state = AppState::for_tests();
    state.runtime_ports.mcp_port = addr.port();
    let temp = std::env::temp_dir().join(format!("codivex-loadtest-{}", std::process::id()));
    std::fs::create_dir_all(&temp)?;
    state.cwd = temp.clone();
    let scoped_project = "/tmp/load-test-project";
    common::projects::write_selected_project(&temp, scoped_project)?;
    common::projects::save_project_index(
        &temp,
        &common::projects::IndexedProject {
            project_path: scoped_project.to_string(),
            files_scanned: 1,
            chunks_extracted: 1,
            indexed_at_unix: 1,
            chunks: vec![common::projects::IndexedChunk {
                file: "src/date.rs".to_string(),
                symbol: Some("iso_to_date".to_string()),
                start_line: 1,
                end_line: 3,
                content: "fn iso_to_date(input: &str) -> String { input.to_string() }".to_string(),
            }],
        },
    )?;

    let router = app::router(state);
    let handle = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            eprintln!("load test local server failed: {err}");
        }
    });
    Ok(LocalServer {
        base_url: format!("http://{addr}"),
        handle,
    })
}

async fn run_api_phase(
    client: &Client,
    base_url: &str,
    cfg: LoadTestConfig,
) -> anyhow::Result<PhaseReport> {
    let semaphore = Arc::new(Semaphore::new(cfg.api_concurrency.max(1)));
    let latencies = Arc::new(Mutex::new(Vec::<f64>::with_capacity(cfg.api_requests)));
    let successes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let phase_start = Instant::now();

    let mut joins = Vec::with_capacity(cfg.api_requests);
    for i in 0..cfg.api_requests {
        let permit = semaphore.clone().acquire_owned().await?;
        let client = client.clone();
        let latencies = latencies.clone();
        let successes = successes.clone();
        let failures = failures.clone();
        let endpoint = format!("{base_url}/mcp");
        joins.push(tokio::spawn(async move {
            let _permit = permit;
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": i as u64,
                "method": "searchCode",
                "params": {
                    "query": "iso to date",
                    "top_k": 5
                }
            });
            let started = Instant::now();
            match client.post(&endpoint).json(&request).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) if body.get("result").is_some() => {
                            successes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            latencies
                                .lock()
                                .await
                                .push(started.elapsed().as_secs_f64() * 1000.0);
                        }
                        _ => {
                            failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
                _ => {
                    failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }));
    }

    for join in joins {
        join.await.context("API load worker panicked")?;
    }

    let elapsed = phase_start.elapsed().as_secs_f64();
    let successes = successes.load(std::sync::atomic::Ordering::Relaxed);
    let failures = failures.load(std::sync::atomic::Ordering::Relaxed);
    let mut l = latencies.lock().await;
    Ok(PhaseReport {
        attempts: cfg.api_requests,
        successes,
        failures,
        throughput_ops_per_sec: if elapsed > 0.0 {
            successes as f64 / elapsed
        } else {
            0.0
        },
        latency_ms: summarize_latencies(&mut l),
    })
}

async fn run_sse_phase(
    client: &Client,
    base_url: &str,
    cfg: LoadTestConfig,
) -> anyhow::Result<PhaseReport> {
    let semaphore = Arc::new(Semaphore::new(cfg.sse_concurrency.max(1)));
    let latencies = Arc::new(Mutex::new(Vec::<f64>::with_capacity(cfg.sse_streams)));
    let successes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let phase_start = Instant::now();

    let mut joins = Vec::with_capacity(cfg.sse_streams);
    for _ in 0..cfg.sse_streams {
        let permit = semaphore.clone().acquire_owned().await?;
        let client = client.clone();
        let latencies = latencies.clone();
        let successes = successes.clone();
        let failures = failures.clone();
        let endpoint = format!("{base_url}/mcp/sse?query=iso%20to%20date&top_k=5");
        joins.push(tokio::spawn(async move {
            let _permit = permit;
            let started = Instant::now();
            match client.get(&endpoint).send().await {
                Ok(resp) if resp.status().is_success() => match resp.text().await {
                    Ok(body) if body.contains("event: done") && body.contains("event: result") => {
                        successes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        latencies
                            .lock()
                            .await
                            .push(started.elapsed().as_secs_f64() * 1000.0);
                    }
                    _ => {
                        failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                },
                _ => {
                    failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }));
    }

    for join in joins {
        join.await.context("SSE load worker panicked")?;
    }

    let elapsed = phase_start.elapsed().as_secs_f64();
    let successes = successes.load(std::sync::atomic::Ordering::Relaxed);
    let failures = failures.load(std::sync::atomic::Ordering::Relaxed);
    let mut l = latencies.lock().await;
    Ok(PhaseReport {
        attempts: cfg.sse_streams,
        successes,
        failures,
        throughput_ops_per_sec: if elapsed > 0.0 {
            successes as f64 / elapsed
        } else {
            0.0
        },
        latency_ms: summarize_latencies(&mut l),
    })
}

fn summarize_latencies(values: &mut [f64]) -> LatencyReport {
    if values.is_empty() {
        return LatencyReport {
            min: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            max: 0.0,
            avg: 0.0,
        };
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = values.iter().sum();
    LatencyReport {
        min: values[0],
        p50: percentile(values, 0.50),
        p95: percentile(values, 0.95),
        p99: percentile(values, 0.99),
        max: values[values.len() - 1],
        avg: sum / values.len() as f64,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}
