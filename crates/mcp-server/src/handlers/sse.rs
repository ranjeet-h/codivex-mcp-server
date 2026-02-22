use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::StreamExt;
use serde::Deserialize;
use std::time::Instant;
use tokio_stream::wrappers::IntervalStream;

use crate::{
    handlers::auth::is_authorized, services::search::scoped_project_results, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    pub project: Option<String>,
}

fn default_top_k() -> usize {
    5
}

pub async fn sse_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(SearchQuery {
        query,
        top_k,
        project,
    }): Query<SearchQuery>,
) -> axum::response::Response {
    if !is_authorized(&headers, &state) {
        let stream = futures::stream::once(async {
            Ok::<Event, Infallible>(Event::default().event("error").data(format!(
                "{{\"status\":{}}}",
                StatusCode::UNAUTHORIZED.as_u16()
            )))
        });
        return Sse::new(Box::pin(stream))
            .keep_alive(KeepAlive::default())
            .into_response();
    }

    metrics::counter!("mcp_sse_requests_total").increment(1);
    let started = Instant::now();
    let scope = project
        .filter(|p| !p.trim().is_empty())
        .or_else(|| {
            headers
                .get("x-codivex-project")
                .and_then(|h| h.to_str().ok())
                .map(str::to_string)
        })
        .or_else(|| common::projects::read_selected_project(&state.cwd))
        .map(|scope| resolve_project_scope(&state.cwd, &scope));
    let Some(scope) = scope else {
        let stream = futures::stream::once(async {
            Ok::<Event, Infallible>(
                Event::default()
                    .event("error")
                    .data("{\"status\":400,\"message\":\"project scope required\"}"),
            )
        });
        return Sse::new(Box::pin(stream))
            .keep_alive(KeepAlive::default())
            .into_response();
    };

    let items = scoped_project_results(&state.cwd, &scope, &query, top_k)
        .await
        .unwrap_or_default();
    state
        .record_search_latency_ms(started.elapsed().as_millis())
        .await;
    if items.is_empty() {
        let stream = futures::stream::once(async {
            Ok::<Event, Infallible>(
                Event::default()
                    .event("error")
                    .data("{\"status\":404,\"message\":\"no indexed data or no matches\"}"),
            )
        });
        return Sse::new(Box::pin(stream))
            .keep_alive(KeepAlive::default())
            .into_response();
    }
    let ticker = IntervalStream::new(tokio::time::interval(Duration::from_millis(120)));
    let stream = ticker.take(items.len()).enumerate().map(move |(idx, _)| {
        let item = &items[idx];
        let payload = serde_json::to_string(item)
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string());
        Ok::<Event, Infallible>(Event::default().event("result").data(payload))
    });
    let done = futures::stream::once(async {
        Ok::<Event, Infallible>(
            Event::default()
                .event("done")
                .data("{\"status\":\"complete\"}"),
        )
    });
    let stream = stream.chain(done);

    Sse::new(Box::pin(stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn resolve_project_scope(cwd: &std::path::Path, scope: &str) -> String {
    let requested = std::path::Path::new(scope);
    if requested.is_absolute() {
        return requested.display().to_string();
    }
    let from_cwd = cwd.join(scope);
    if from_cwd.exists() {
        return from_cwd.display().to_string();
    }
    for root in configured_project_roots(cwd) {
        let candidate = root.join(scope);
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }
    from_cwd.display().to_string()
}

fn configured_project_roots(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots = vec![cwd.to_path_buf()];
    if let Ok(raw) = std::env::var("CODIVEX_PROJECT_ROOTS") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        roots.extend(
            raw.split(sep)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from),
        );
    }
    roots
}
