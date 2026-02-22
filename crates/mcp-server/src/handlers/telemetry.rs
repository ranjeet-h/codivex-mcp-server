use std::{convert::Infallible, path::Path, time::Duration};

use axum::{
    Json,
    extract::State,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::StreamExt;
use serde::Serialize;
use tokio_stream::wrappers::IntervalStream;

use crate::state::{AppState, ProjectRuntimeStatus};

#[derive(Debug, Clone, Serialize)]
pub struct TelemetrySnapshot {
    pub selected_project: Option<String>,
    pub queue_depth: u64,
    pub chunks_indexed: u64,
    pub index_size_bytes: u64,
    pub latency_p50_ms: u128,
    pub latency_p95_ms: u128,
    pub projects: Vec<ProjectCatalogSnapshot>,
    pub runtime_watchers: Vec<ProjectRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectCatalogSnapshot {
    pub project_path: String,
    pub files_scanned: usize,
    pub chunks_extracted: usize,
    pub indexed_at_unix: u64,
    pub index_size_bytes: u64,
}

pub async fn telemetry_handler(State(state): State<AppState>) -> Json<TelemetrySnapshot> {
    Json(build_snapshot(&state).await)
}

pub async fn telemetry_sse_handler(State(state): State<AppState>) -> impl IntoResponse {
    let ticker = IntervalStream::new(tokio::time::interval(Duration::from_secs(1)));
    let stream = ticker.then(move |_| {
        let state = state.clone();
        async move {
            let snapshot = build_snapshot(&state).await;
            let payload = serde_json::to_string(&snapshot)
                .unwrap_or_else(|_| "{\"error\":\"telemetry serialization failed\"}".to_string());
            Ok::<Event, Infallible>(Event::default().event("telemetry").data(payload))
        }
    });
    Sse::new(Box::pin(stream)).keep_alive(KeepAlive::default())
}

async fn build_snapshot(state: &AppState) -> TelemetrySnapshot {
    let telemetry = state.indexer_telemetry.snapshot();
    let selected_project = common::projects::read_selected_project(&state.cwd);
    let catalog = common::projects::read_catalog(&state.cwd);
    let runtime_watchers = state.indexing_runtime.snapshot().await;
    let (latency_p50_ms, latency_p95_ms) = state.search_latency_percentiles_ms().await;

    let projects = catalog
        .projects
        .into_iter()
        .map(|entry| ProjectCatalogSnapshot {
            index_size_bytes: project_storage_size(&state.cwd, &entry.project_path),
            project_path: entry.project_path,
            files_scanned: entry.files_scanned,
            chunks_extracted: entry.chunks_extracted,
            indexed_at_unix: entry.indexed_at_unix,
        })
        .collect::<Vec<_>>();

    let index_size_bytes = selected_project
        .as_ref()
        .map(|project| project_storage_size(&state.cwd, project))
        .unwrap_or(0);

    TelemetrySnapshot {
        selected_project,
        queue_depth: telemetry.queue_depth,
        chunks_indexed: telemetry.chunks_indexed,
        index_size_bytes,
        latency_p50_ms,
        latency_p95_ms,
        projects,
        runtime_watchers,
    }
}

fn project_storage_size(cwd: &Path, project_path: &str) -> u64 {
    let storage = common::projects::project_storage_dir(cwd, project_path);
    directory_size(&storage)
}

fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut size = 0u64;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            size = size.saturating_add(directory_size(&p));
        } else if let Ok(meta) = entry.metadata() {
            size = size.saturating_add(meta.len());
        }
    }
    size
}
