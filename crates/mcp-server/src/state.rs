use common::ports::RuntimePorts;
use indexer::telemetry::IndexerTelemetry;
use lru::LruCache;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub metrics: PrometheusHandle,
    pub api_token: Option<String>,
    pub runtime_ports: RuntimePorts,
    pub port_conflicts_resolved: bool,
    pub pid: u32,
    pub cwd: PathBuf,
    pub query_cache: Arc<Mutex<LruCache<String, common::SearchCodeResult>>>,
    pub indexer_telemetry: Arc<IndexerTelemetry>,
    pub indexing_runtime: Arc<IndexingRuntimeState>,
    pub search_latencies_ms: Arc<Mutex<VecDeque<u128>>>,
    shutting_down: Arc<AtomicBool>,
}

impl AppState {
    pub fn from_env(
        runtime_ports: RuntimePorts,
        port_conflicts_resolved: bool,
    ) -> anyhow::Result<Self> {
        let handle = PrometheusBuilder::new().install_recorder()?;
        let api_token = std::env::var("MCP_API_TOKEN").ok();
        Ok(Self {
            metrics: handle,
            api_token,
            runtime_ports,
            port_conflicts_resolved,
            pid: std::process::id(),
            cwd: std::env::current_dir()?,
            query_cache: Arc::new(Mutex::new(LruCache::new(cache_capacity_from_env()))),
            indexer_telemetry: Arc::new(IndexerTelemetry::default()),
            indexing_runtime: Arc::new(IndexingRuntimeState::default()),
            search_latencies_ms: Arc::new(Mutex::new(VecDeque::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn for_tests() -> Self {
        let recorder = PrometheusBuilder::new().build_recorder();
        Self {
            metrics: recorder.handle(),
            api_token: None,
            runtime_ports: RuntimePorts {
                mcp_port: 38080,
                ui_port: 38181,
                metrics_port: Some(38281),
            },
            port_conflicts_resolved: false,
            pid: 1,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            query_cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(128).expect("non-zero"),
            ))),
            indexer_telemetry: Arc::new(IndexerTelemetry::default()),
            indexing_runtime: Arc::new(IndexingRuntimeState::default()),
            search_latencies_ms: Arc::new(Mutex::new(VecDeque::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn record_search_latency_ms(&self, latency_ms: u128) {
        let mut guard = self.search_latencies_ms.lock().await;
        guard.push_back(latency_ms);
        if guard.len() > 1024 {
            let _ = guard.pop_front();
        }
    }

    pub async fn search_latency_percentiles_ms(&self) -> (u128, u128) {
        let guard = self.search_latencies_ms.lock().await;
        if guard.is_empty() {
            return (0, 0);
        }
        let mut values = guard.iter().copied().collect::<Vec<_>>();
        values.sort_unstable();
        let p50 = percentile(&values, 0.50);
        let p95 = percentile(&values, 0.95);
        (p50, p95)
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub async fn persist_runtime_state(&self) -> anyhow::Result<()> {
        let state_root = self.cwd.join(".codivex");
        std::fs::create_dir_all(&state_root)?;
        let snapshot = RuntimeStateSnapshot {
            unix_ms: unix_now_ms(),
            projects: self.indexing_runtime.snapshot().await,
            telemetry: self.indexer_telemetry.snapshot(),
            search_latency_ms: {
                let (p50, p95) = self.search_latency_percentiles_ms().await;
                SearchLatencySnapshot { p50, p95 }
            },
        };
        let target = state_root.join("runtime-state.json");
        std::fs::write(target, serde_json::to_string_pretty(&snapshot)?)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PortDiagnostics {
    pub mcp_port: u16,
    pub ui_port: u16,
    pub metrics_port: Option<u16>,
    pub conflicts_resolved: bool,
    pub pid: u32,
}

fn cache_capacity_from_env() -> NonZeroUsize {
    let parsed = std::env::var("MCP_QUERY_CACHE_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(512)
        .max(1);
    NonZeroUsize::new(parsed).expect("cache capacity max(1) guarantees non-zero")
}

#[derive(Default)]
pub struct IndexingRuntimeState {
    projects: RwLock<HashMap<String, ProjectRuntimeStatus>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectRuntimeStatus {
    pub project_path: String,
    pub active_watcher: bool,
    pub queue_depth: u64,
    pub chunks_indexed: u64,
    pub last_indexed_unix_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeStateSnapshot {
    unix_ms: u64,
    projects: Vec<ProjectRuntimeStatus>,
    telemetry: indexer::telemetry::IndexerTelemetrySnapshot,
    search_latency_ms: SearchLatencySnapshot,
}

#[derive(Debug, Serialize)]
struct SearchLatencySnapshot {
    p50: u128,
    p95: u128,
}

impl IndexingRuntimeState {
    pub async fn mark_watcher_active(&self, project_path: &str, active: bool) {
        let mut guard = self.projects.write().await;
        let entry = guard
            .entry(project_path.to_string())
            .or_insert_with(|| ProjectRuntimeStatus {
                project_path: project_path.to_string(),
                ..ProjectRuntimeStatus::default()
            });
        entry.active_watcher = active;
    }

    pub async fn set_queue_depth(&self, project_path: &str, depth: u64) {
        let mut guard = self.projects.write().await;
        let entry = guard
            .entry(project_path.to_string())
            .or_insert_with(|| ProjectRuntimeStatus {
                project_path: project_path.to_string(),
                ..ProjectRuntimeStatus::default()
            });
        entry.queue_depth = depth;
    }

    pub async fn mark_indexed(&self, project_path: &str, chunk_delta: u64) {
        let mut guard = self.projects.write().await;
        let entry = guard
            .entry(project_path.to_string())
            .or_insert_with(|| ProjectRuntimeStatus {
                project_path: project_path.to_string(),
                ..ProjectRuntimeStatus::default()
            });
        entry.chunks_indexed = entry.chunks_indexed.saturating_add(chunk_delta);
        entry.last_indexed_unix_ms = unix_now_ms();
        entry.last_error = None;
    }

    pub async fn mark_error(&self, project_path: &str, message: String) {
        let mut guard = self.projects.write().await;
        let entry = guard
            .entry(project_path.to_string())
            .or_insert_with(|| ProjectRuntimeStatus {
                project_path: project_path.to_string(),
                ..ProjectRuntimeStatus::default()
            });
        entry.last_error = Some(message);
    }

    pub async fn snapshot(&self) -> Vec<ProjectRuntimeStatus> {
        let guard = self.projects.read().await;
        let mut out = guard.values().cloned().collect::<Vec<_>>();
        out.sort_by(|a, b| a.project_path.cmp(&b.project_path));
        out
    }
}

fn unix_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as u64,
        Err(_) => 0,
    }
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::AppState;

    #[tokio::test]
    async fn persist_runtime_state_writes_snapshot_file() {
        let mut state = AppState::for_tests();
        let base = std::env::temp_dir().join(format!("codivex-state-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("mkdir");
        state.cwd = base.clone();
        state
            .indexing_runtime
            .mark_watcher_active("/tmp/repo", true)
            .await;
        state.record_search_latency_ms(42).await;

        state.persist_runtime_state().await.expect("persist");

        let target = base.join(".codivex/runtime-state.json");
        let raw = std::fs::read_to_string(target).expect("runtime-state");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert!(parsed["projects"].is_array());
        assert!(parsed["telemetry"].is_object());
    }
}
