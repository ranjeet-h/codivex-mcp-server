use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::Path;
use std::time::Instant;

use axum::{
    Json, Router,
    extract::{Query, State},
    response::{
        Html,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use common::ports::RuntimePorts;
use common::projects::{self, IndexedChunk, IndexedProject};
use common::{CodeChunk, OpenLocationParams, RpcRequest, SearchCodeParams};
use dioxus::prelude::*;
use embeddings::{EmbeddingConfig, EmbeddingEngine};
use qdrant_client::Qdrant;
use search_core::lexical::TantivyLexicalIndex;
use search_core::vector::{
    QdrantVectorStore, QuantizationMode as VectorQuantizationMode, VectorSearchConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::IntervalStream;

use crate::ui::AdminPage;

#[derive(Clone)]
struct UiState {
    ports: RuntimePorts,
    pid: u32,
    cwd: std::path::PathBuf,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct UiDiagnostics {
    ui_port: u16,
    mcp_port: u16,
    metrics_port: Option<u16>,
    pid: u32,
}

pub async fn run_ui_server() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let preferred_mcp = std::env::var("MCP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(38080);
    let preferred_ui = std::env::var("UI_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(38181);
    let ports = resolve_ui_runtime_ports(&cwd, preferred_mcp, preferred_ui, Some(38281))?;

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), ports.ui_port);
    let state = UiState {
        ports: ports.clone(),
        pid: std::process::id(),
        cwd,
        http: reqwest::Client::new(),
    };

    let app = build_router(state);

    println!("ui-dioxus listening on http://{addr}/admin");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn resolve_ui_runtime_ports(
    cwd: &Path,
    preferred_mcp: u16,
    preferred_ui: u16,
    preferred_metrics: Option<u16>,
) -> anyhow::Result<RuntimePorts> {
    let state_path = cwd.join(".codivex").join("runtime-ports.json");
    let existing = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<RuntimePorts>(&raw).ok());

    let mcp_port = existing
        .as_ref()
        .map(|p| p.mcp_port)
        .unwrap_or(preferred_mcp);
    let preferred_ui_port = existing.as_ref().map(|p| p.ui_port).unwrap_or(preferred_ui);
    let ui_port = if preferred_ui_port != mcp_port && port_available(preferred_ui_port) {
        preferred_ui_port
    } else {
        find_open_port(38181, 38280, mcp_port)?
    };
    let metrics_port = existing
        .as_ref()
        .and_then(|p| p.metrics_port)
        .or(preferred_metrics);

    let ports = RuntimePorts {
        mcp_port,
        ui_port,
        metrics_port,
    };
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&state_path, serde_json::to_string_pretty(&ports)?)?;
    Ok(ports)
}

fn port_available(port: u16) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    TcpListener::bind(addr).is_ok()
}

fn find_open_port(start: u16, end: u16, excluded: u16) -> anyhow::Result<u16> {
    for port in start..=end {
        if port == excluded {
            continue;
        }
        if port_available(port) {
            return Ok(port);
        }
    }
    anyhow::bail!("no available UI port in range {start}..={end}")
}

fn build_router(state: UiState) -> Router {
    Router::new()
        .route("/", get(admin_html))
        .route("/admin", get(admin_html))
        .route("/health", get(|| async { "ok" }))
        .route("/port-diagnostics", get(port_diagnostics))
        .route("/api/search", post(api_search))
        .route("/api/sse", get(api_sse))
        .route("/api/telemetry", get(api_telemetry))
        .route("/api/telemetry/sse", get(api_telemetry_sse))
        .route("/api/open-location", post(api_open_location))
        .route("/api/smoke-test", post(api_smoke_test))
        .route("/api/projects/scan", post(api_projects_scan))
        .route("/api/project/select", post(api_project_select))
        .route("/api/index/action", post(api_index_action))
        .route("/api/agent-test", post(api_agent_test))
        .with_state(state)
}

async fn admin_html(State(state): State<UiState>) -> Html<String> {
    let mcp_endpoint = format!("http://127.0.0.1:{}/mcp", state.ports.mcp_port);
    let ui_endpoint = format!("http://127.0.0.1:{}/admin", state.ports.ui_port);
    let rendered = render(rsx! {
        AdminPage {
            mcp_endpoint: mcp_endpoint.clone(),
            ui_endpoint: ui_endpoint.clone(),
        }
    });
    Html(format!(
        r#"<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <title>Codivex Admin</title>
</head>
<body>{}</body>
<script>
const byId = (id) => document.getElementById(id);
const folderPicker = byId('folder-picker');
folderPicker.setAttribute('webkitdirectory', '');
folderPicker.setAttribute('directory', '');
folderPicker.setAttribute('mozdirectory', '');

function renderResults(items) {{
  const body = byId('result-tbody');
  body.innerHTML = '';
  for (const item of items) {{
    const tr = document.createElement('tr');
    tr.innerHTML = `<td style="border-bottom:1px solid #eee;padding:8px;">${{item.file}}</td><td style="border-bottom:1px solid #eee;padding:8px;">${{item.function}}</td><td style="border-bottom:1px solid #eee;padding:8px;">${{item.start_line}}-${{item.end_line}}</td>`;
    body.appendChild(tr);
  }}
}}

function renderCatalog(projects) {{
  const body = byId('project-catalog-body');
  body.innerHTML = '';
  for (const project of projects || []) {{
    const tr = document.createElement('tr');
    tr.innerHTML = `<td style="border-bottom:1px solid #eee;padding:8px;">${{project.project_path}}</td><td style="border-bottom:1px solid #eee;padding:8px;">${{project.files_scanned}}</td><td style="border-bottom:1px solid #eee;padding:8px;">${{project.chunks_extracted}}</td><td style="border-bottom:1px solid #eee;padding:8px;">${{project.indexed_at_unix}}</td>`;
    body.appendChild(tr);
  }}
}}

function formatBytes(bytes) {{
  const value = Number(bytes || 0);
  if (value < 1024) return `${{value}} B`;
  if (value < 1024 * 1024) return `${{(value / 1024).toFixed(1)}} KB`;
  if (value < 1024 * 1024 * 1024) return `${{(value / (1024 * 1024)).toFixed(1)}} MB`;
  return `${{(value / (1024 * 1024 * 1024)).toFixed(1)}} GB`;
}}

async function selectProject(pathValue) {{
  const path = (pathValue || '').trim();
  if (!path) {{
    byId('selected-project-status').textContent = 'Selected project: provide a path first';
    return;
  }}
  const res = await fetch('/api/project/select', {{
    method: 'POST',
    headers: {{ 'content-type': 'application/json' }},
    body: JSON.stringify({{ path }})
  }});
  const data = await res.json();
  const selected = data.selected_path || path;
  byId('selected-project-status').textContent = `Selected project: ${{selected}}`;
  byId('project-path-input').value = selected;
}}

async function runSearch() {{
  const query = byId('search-query').value.trim();
  const topK = Number(byId('search-topk').value || '5');
  if (!query) {{
    byId('search-status').textContent = 'Status: query cannot be empty';
    return;
  }}
  byId('search-status').textContent = 'Status: running search...';

  const rpcRes = await fetch('/api/search', {{
    method: 'POST',
    headers: {{ 'content-type': 'application/json' }},
    body: JSON.stringify({{ query, top_k: topK }})
  }});
  const rpcData = await rpcRes.json();
  const items = rpcData?.result?.items || [];
  renderResults(items);

  const sseRes = await fetch(`/api/sse?query=${{encodeURIComponent(query)}}&top_k=${{topK}}`);
  const sseText = await sseRes.text();
  byId('sse-stream-output').textContent = sseText;

  byId('search-status').textContent = 'Status: search complete';
}}

byId('btn-search').addEventListener('click', runSearch);

byId('btn-select-repo').addEventListener('click', async () => {{
  if (window.showDirectoryPicker) {{
    try {{
      const handle = await window.showDirectoryPicker();
      const name = handle?.name || '';
      if (!name) {{
        byId('selected-project-status').textContent = 'Selected project: unable to read selection';
        return;
      }}
      byId('project-path-input').value = name;
      await selectProject(name);
      return;
    }} catch (_) {{
      // User cancelled or API unavailable in this browser; fallback below.
    }}
  }}
  folderPicker.click();
}});

folderPicker.addEventListener('change', async (event) => {{
  const files = event.target.files || [];
  if (!files.length) {{
    return;
  }}
  const first = files[0];
  const rel = first.webkitRelativePath || '';
  const folderName = rel.split('/')[0] || '';
  if (!folderName) {{
    byId('selected-project-status').textContent = 'Selected project: unable to read selection';
    return;
  }}
  byId('project-path-input').value = folderName;
  await selectProject(folderName);
}});

async function runIndexAction(action) {{
  const path = byId('project-path-input').value.trim();
  byId('index-action-status').textContent = `Index status: running ${{action}}...`;
  const res = await fetch('/api/index/action', {{
    method: 'POST',
    headers: {{ 'content-type': 'application/json' }},
    body: JSON.stringify({{ action, path }})
  }});
  const data = await res.json();
  byId('index-action-status').textContent =
    `Index status: ${{data.action}} complete (files=${{data.files_scanned}}, chunks=${{data.chunks_extracted}}, ms=${{data.duration_ms}})`;
  if (data.path) {{
    byId('selected-project-status').textContent = `Selected project: ${{data.path}}`;
    byId('project-path-input').value = data.path;
  }}
}}

byId('btn-apply-path').addEventListener('click', async () => {{
  await selectProject(byId('project-path-input').value);
}});

byId('btn-start-index').addEventListener('click', () => runIndexAction('start'));
byId('btn-reindex').addEventListener('click', () => runIndexAction('reindex'));
byId('btn-clear-index').addEventListener('click', () => runIndexAction('clear'));

async function loadTelemetrySnapshot() {{
  const res = await fetch('/api/telemetry');
  const telemetry = await res.json();
  updateTelemetry(telemetry);
}}

function updateTelemetry(telemetry) {{
  if (!telemetry) return;
  if (telemetry.selected_project) {{
    byId('selected-project-status').textContent = `Selected project: ${{telemetry.selected_project}}`;
  }}
  byId('health-queue-depth').textContent = String(telemetry.queue_depth || 0);
  byId('health-chunks-indexed').textContent = String(telemetry.chunks_indexed || 0);
  byId('health-index-size').textContent = formatBytes(telemetry.index_size_bytes || 0);
  byId('health-latency').textContent = `${{telemetry.latency_p50_ms || 0}}ms / ${{telemetry.latency_p95_ms || 0}}ms`;
  byId('runtime-watchers').textContent = JSON.stringify(telemetry.runtime_watchers || [], null, 2);
  renderCatalog(telemetry.projects || []);
}}

const telemetryEvents = new EventSource('/api/telemetry/sse');
telemetryEvents.addEventListener('telemetry', (event) => {{
  try {{
    updateTelemetry(JSON.parse(event.data));
  }} catch (_) {{}}
}});
telemetryEvents.addEventListener('error', () => {{
  byId('index-action-status').textContent = 'Index status: telemetry stream disconnected';
}});

loadTelemetrySnapshot().catch(() => {{
  byId('index-action-status').textContent = 'Index status: telemetry unavailable';
}});
</script>
</html>"#,
        rendered
    ))
}

fn render(component: Element) -> String {
    use dioxus_ssr::render_element;
    render_element(component)
}

#[derive(Debug, Deserialize)]
struct SearchApiRequest {
    query: String,
    top_k: usize,
}

#[derive(Debug, Deserialize)]
struct OpenLocationApiRequest {
    path: String,
    line_start: usize,
    line_end: usize,
}

#[derive(Debug, Deserialize)]
struct SseApiQuery {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}

#[derive(Debug, Serialize)]
struct SmokeTestResult {
    rpc_ok: bool,
    sse_result_ok: bool,
    sse_done_ok: bool,
    open_location_ok: bool,
}

#[derive(Debug, Deserialize)]
struct ProjectSelectRequest {
    path: String,
}

#[derive(Debug, Serialize)]
struct ProjectSelectResponse {
    selected_path: String,
}

#[derive(Debug, Serialize)]
struct ProjectScanResponse {
    projects: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct IndexActionRequest {
    action: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct IndexActionResponse {
    action: String,
    path: String,
    files_scanned: usize,
    chunks_extracted: usize,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct AgentTestReport {
    selected_project: Option<String>,
    exact_symbol_latency_ms: u128,
    semantic_latency_ms: u128,
    open_location_latency_ms: u128,
    sse_latency_ms: u128,
    exact_symbol_ok: bool,
    semantic_ok: bool,
    open_location_ok: bool,
    sse_done_ok: bool,
}

fn default_top_k() -> usize {
    5
}

async fn api_search(
    State(state): State<UiState>,
    Json(req): Json<SearchApiRequest>,
) -> Json<serde_json::Value> {
    let scope = project_scope(&state);
    let payload = RpcRequest {
        jsonrpc: "2.0".to_string(),
        id: common::RpcId::Number(1),
        method: "searchCode".to_string(),
        params: serde_json::to_value(SearchCodeParams {
            query: req.query,
            top_k: req.top_k.max(1),
            repo_filter: scope,
        })
        .unwrap_or_else(|_| json!({})),
    };
    proxy_rpc(&state, payload).await
}

async fn api_open_location(
    State(state): State<UiState>,
    Json(req): Json<OpenLocationApiRequest>,
) -> Json<serde_json::Value> {
    let payload = RpcRequest {
        jsonrpc: "2.0".to_string(),
        id: common::RpcId::Number(2),
        method: "openLocation".to_string(),
        params: serde_json::to_value(OpenLocationParams {
            path: req.path,
            line_start: req.line_start,
            line_end: req.line_end,
        })
        .unwrap_or_else(|_| json!({})),
    };
    proxy_rpc(&state, payload).await
}

async fn proxy_rpc(state: &UiState, payload: RpcRequest) -> Json<serde_json::Value> {
    let endpoint = format!("http://127.0.0.1:{}/mcp", state.ports.mcp_port);
    let mut req = state.http.post(endpoint).json(&payload);
    if let Some(scope) = project_scope(state) {
        req = req.header("x-codivex-project", scope);
    }
    match req.send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => Json(json),
            Err(_) => Json(json!({
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": { "code": -32603, "message": "invalid proxy response" }
            })),
        },
        Err(_) => Json(json!({
            "jsonrpc": "2.0",
            "id": payload.id,
            "error": { "code": -32603, "message": "mcp unavailable" }
        })),
    }
}

async fn api_sse(State(state): State<UiState>, Query(q): Query<SseApiQuery>) -> String {
    let endpoint = format!(
        "http://127.0.0.1:{}/mcp/sse?query={}&top_k={}",
        state.ports.mcp_port,
        urlencoding::encode(&q.query),
        q.top_k.max(1)
    );
    let mut req = state.http.get(endpoint);
    if let Some(scope) = project_scope(&state) {
        req = req.header("x-codivex-project", scope);
    }
    match req.send().await {
        Ok(resp) => resp.text().await.unwrap_or_else(|_| String::new()),
        Err(_) => String::new(),
    }
}

async fn api_telemetry(State(state): State<UiState>) -> Json<serde_json::Value> {
    let endpoint = format!("http://127.0.0.1:{}/telemetry", state.ports.mcp_port);
    match state.http.get(endpoint).send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(payload) => Json(payload),
            Err(_) => Json(json!({"error": "invalid telemetry response"})),
        },
        Err(_) => Json(json!({"error": "telemetry unavailable"})),
    }
}

async fn api_telemetry_sse(
    State(state): State<UiState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let ticker = IntervalStream::new(tokio::time::interval(std::time::Duration::from_secs(1)));
    let stream = ticker.then(move |_| {
        let state = state.clone();
        async move {
            let payload = api_telemetry(State(state)).await.0;
            let body = serde_json::to_string(&payload)
                .unwrap_or_else(|_| "{\"error\":\"telemetry serialization failed\"}".to_string());
            Ok::<Event, Infallible>(Event::default().event("telemetry").data(body))
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn api_smoke_test(State(state): State<UiState>) -> Json<SmokeTestResult> {
    let scope = project_scope(&state);
    let search = proxy_rpc(
        &state,
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: common::RpcId::Number(3),
            method: "searchCode".to_string(),
            params: serde_json::to_value(SearchCodeParams {
                query: "iso_to_date".to_string(),
                top_k: 2,
                repo_filter: scope.clone(),
            })
            .unwrap_or_else(|_| json!({})),
        },
    )
    .await
    .0;

    let sse_text = api_sse(
        State(state.clone()),
        Query(SseApiQuery {
            query: "iso to date".to_string(),
            top_k: 2,
        }),
    )
    .await;

    let open_target = first_result_location(&search);
    let open = if let Some((path, start, end)) = open_target {
        proxy_rpc(
            &state,
            RpcRequest {
                jsonrpc: "2.0".to_string(),
                id: common::RpcId::Number(4),
                method: "openLocation".to_string(),
                params: serde_json::to_value(OpenLocationParams {
                    path,
                    line_start: start,
                    line_end: end,
                })
                .unwrap_or_else(|_| json!({})),
            },
        )
        .await
        .0
    } else {
        json!({})
    };

    Json(SmokeTestResult {
        rpc_ok: search.get("result").is_some(),
        sse_result_ok: sse_text.contains("event: result"),
        sse_done_ok: sse_text.contains("event: done"),
        open_location_ok: open.get("result").and_then(|v| v.get("path")).is_some(),
    })
}

async fn api_projects_scan() -> Json<ProjectScanResponse> {
    let mut projects = Vec::new();
    for root in configured_project_roots() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path.join(".git").exists()
                    || path.join("Cargo.toml").exists()
                    || path.join("package.json").exists()
                {
                    projects.push(path.display().to_string());
                }
            }
        }
    }
    projects.sort();
    Json(ProjectScanResponse { projects })
}

async fn api_project_select(Json(req): Json<ProjectSelectRequest>) -> Json<ProjectSelectResponse> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let selected = resolve_project_path(req.path.trim(), &cwd);
    let _ = projects::write_selected_project(&cwd, &selected);
    Json(ProjectSelectResponse {
        selected_path: selected,
    })
}

async fn api_index_action(
    State(state): State<UiState>,
    Json(req): Json<IndexActionRequest>,
) -> Json<IndexActionResponse> {
    let action = req.action.trim().to_lowercase();
    let repo_path = if req.path.trim().is_empty() {
        projects::read_selected_project(&state.cwd).unwrap_or_default()
    } else {
        resolve_project_path(req.path.trim(), &state.cwd)
    };
    let _ = projects::write_selected_project(&state.cwd, &repo_path);

    let started = Instant::now();
    let cwd = state.cwd.clone();
    let (files_scanned, chunks_extracted) = run_index_action(&cwd, &action, Path::new(&repo_path))
        .await
        .unwrap_or((0, 0));
    Json(IndexActionResponse {
        action,
        path: repo_path,
        files_scanned,
        chunks_extracted,
        duration_ms: started.elapsed().as_millis(),
    })
}

async fn run_index_action(cwd: &Path, action: &str, repo: &Path) -> anyhow::Result<(usize, usize)> {
    let repo_path = repo.display().to_string();
    if action == "clear" {
        projects::remove_project_index(cwd, &repo_path)?;
        if let Some(client) = qdrant_client_from_env()? {
            let _ = client
                .delete_collection(projects::project_vector_collection(&repo_path))
                .await;
        }
        return Ok((0, 0));
    }

    let repo = repo.to_path_buf();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || -> anyhow::Result<IndexActionOutput> {
        let files = indexer::scanner::scan_source_files(&repo);
        let mut chunk_count = 0usize;
        let mut indexed_chunks = Vec::new();
        let mut code_chunks = Vec::new();

        for path in &files {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(chunks) =
                    indexer::extract_chunks_for_file(path.to_string_lossy().as_ref(), &content)
                {
                    chunk_count += chunks.len();
                    for chunk in chunks {
                        indexed_chunks.push(IndexedChunk {
                            file: chunk.file_path.clone(),
                            symbol: chunk.symbol.clone(),
                            start_line: chunk.start_line,
                            end_line: chunk.end_line,
                            content: chunk.content.clone(),
                        });
                        code_chunks.push(chunk);
                    }
                }
            }
        }

        let project_path = repo.display().to_string();
        let indexed = IndexedProject {
            project_path: repo.display().to_string(),
            files_scanned: files.len(),
            chunks_extracted: chunk_count,
            indexed_at_unix: unix_now(),
            chunks: indexed_chunks,
        };
        projects::save_project_index(&cwd, &indexed)?;
        persist_tantivy_index(&cwd, &project_path, &code_chunks)?;

        Ok(IndexActionOutput {
            project_path,
            files_scanned: files.len(),
            chunks_extracted: chunk_count,
            code_chunks,
        })
    })
    .await??;

    persist_qdrant_vectors(&output).await?;
    Ok((output.files_scanned, output.chunks_extracted))
}

#[derive(Debug)]
struct IndexActionOutput {
    project_path: String,
    files_scanned: usize,
    chunks_extracted: usize,
    code_chunks: Vec<CodeChunk>,
}

fn persist_tantivy_index(
    cwd: &Path,
    project_path: &str,
    chunks: &[CodeChunk],
) -> anyhow::Result<()> {
    let index_dir = projects::project_lexical_index_dir(cwd, project_path);
    let mut index = TantivyLexicalIndex::open_or_create_on_disk(&index_dir)?;
    index.reset()?;
    for chunk in chunks {
        index.add_chunk(chunk)?;
    }
    index.commit()?;
    Ok(())
}

async fn persist_qdrant_vectors(output: &IndexActionOutput) -> anyhow::Result<()> {
    if output.code_chunks.is_empty() {
        return Ok(());
    }
    let Some(client) = qdrant_client_from_env()? else {
        return Ok(());
    };

    let texts = output
        .code_chunks
        .iter()
        .map(|chunk| chunk.content.clone())
        .collect::<Vec<_>>();
    let embedding_cfg = EmbeddingConfig::default();
    let engine = EmbeddingEngine::new(embedding_cfg.clone());
    let vectors = engine.embed_batch(&texts)?;
    if vectors.is_empty() {
        return Ok(());
    }

    let mut cfg = VectorSearchConfig {
        collection: projects::project_vector_collection(&output.project_path),
        ..VectorSearchConfig::default()
    };
    cfg.vector_dim = vectors[0].len();
    cfg.quantization = to_vector_quantization_mode(embedding_cfg.quantization);
    let store = QdrantVectorStore::new(cfg);
    store.ensure_collection(&client).await?;
    store
        .upsert_chunks(&client, &output.code_chunks, &vectors)
        .await?;
    Ok(())
}

fn qdrant_client_from_env() -> anyhow::Result<Option<Qdrant>> {
    let url = std::env::var("QDRANT_URL").ok();
    let Some(url) = url.filter(|v| !v.trim().is_empty()) else {
        return Ok(None);
    };
    Ok(Some(Qdrant::from_url(&url).build()?))
}

fn to_vector_quantization_mode(mode: embeddings::QuantizationMode) -> VectorQuantizationMode {
    match mode {
        embeddings::QuantizationMode::None => VectorQuantizationMode::None,
        embeddings::QuantizationMode::Int8 => VectorQuantizationMode::Int8,
        embeddings::QuantizationMode::UInt8 => VectorQuantizationMode::UInt8,
    }
}

async fn api_agent_test(State(state): State<UiState>) -> Json<AgentTestReport> {
    let scope = project_scope(&state);
    let exact_started = Instant::now();
    let exact = proxy_rpc(
        &state,
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: common::RpcId::Number(21),
            method: "searchCode".to_string(),
            params: serde_json::to_value(SearchCodeParams {
                query: "iso_to_date".to_string(),
                top_k: 5,
                repo_filter: scope.clone(),
            })
            .unwrap_or_else(|_| json!({})),
        },
    )
    .await
    .0;
    let exact_ms = exact_started.elapsed().as_millis();

    let semantic_started = Instant::now();
    let semantic = proxy_rpc(
        &state,
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: common::RpcId::Number(22),
            method: "searchCode".to_string(),
            params: serde_json::to_value(SearchCodeParams {
                query: "convert iso string to date".to_string(),
                top_k: 5,
                repo_filter: scope,
            })
            .unwrap_or_else(|_| json!({})),
        },
    )
    .await
    .0;
    let semantic_ms = semantic_started.elapsed().as_millis();

    let open_target = first_result_location(&exact).or_else(|| first_result_location(&semantic));
    let open_started = Instant::now();
    let open = if let Some((path, start, end)) = open_target {
        proxy_rpc(
            &state,
            RpcRequest {
                jsonrpc: "2.0".to_string(),
                id: common::RpcId::Number(23),
                method: "openLocation".to_string(),
                params: serde_json::to_value(OpenLocationParams {
                    path,
                    line_start: start,
                    line_end: end,
                })
                .unwrap_or_else(|_| json!({})),
            },
        )
        .await
        .0
    } else {
        json!({})
    };
    let open_ms = open_started.elapsed().as_millis();

    let sse_started = Instant::now();
    let sse_text = api_sse(
        State(state.clone()),
        Query(SseApiQuery {
            query: "iso to date".to_string(),
            top_k: 5,
        }),
    )
    .await;
    let sse_ms = sse_started.elapsed().as_millis();

    Json(AgentTestReport {
        selected_project: projects::read_selected_project(&state.cwd),
        exact_symbol_latency_ms: exact_ms,
        semantic_latency_ms: semantic_ms,
        open_location_latency_ms: open_ms,
        sse_latency_ms: sse_ms,
        exact_symbol_ok: exact.get("result").is_some(),
        semantic_ok: semantic.get("result").is_some(),
        open_location_ok: open.get("result").and_then(|v| v.get("path")).is_some(),
        sse_done_ok: sse_text.contains("event: done"),
    })
}

fn resolve_project_path(raw: &str, cwd: &Path) -> String {
    let candidate = Path::new(raw);
    if candidate.is_absolute() && candidate.exists() {
        return raw.to_string();
    }
    let from_cwd = cwd.join(raw);
    if from_cwd.exists() {
        return from_cwd.display().to_string();
    }
    for root in configured_project_roots() {
        let by_name = root.join(raw);
        if by_name.exists() {
            return by_name.display().to_string();
        }
    }
    from_cwd.display().to_string()
}

fn configured_project_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
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

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

async fn port_diagnostics(State(state): State<UiState>) -> Json<UiDiagnostics> {
    Json(UiDiagnostics {
        ui_port: state.ports.ui_port,
        mcp_port: state.ports.mcp_port,
        metrics_port: state.ports.metrics_port,
        pid: state.pid,
    })
}

fn project_scope(state: &UiState) -> Option<String> {
    projects::read_selected_project(&state.cwd)
}

fn first_result_location(payload: &serde_json::Value) -> Option<(String, usize, usize)> {
    let item = payload
        .get("result")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())?;
    let path = item.get("file")?.as_str()?.to_string();
    let start = item.get("start_line")?.as_u64()? as usize;
    let end = item.get("end_line")?.as_u64()? as usize;
    Some((path, start, end))
}

#[cfg(test)]
mod tests {
    use super::{UiState, build_router};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use common::ports::RuntimePorts;
    use tower::ServiceExt;

    #[tokio::test]
    async fn admin_route_exists() {
        let app = build_router(UiState {
            ports: RuntimePorts {
                mcp_port: 38080,
                ui_port: 38181,
                metrics_port: Some(38281),
            },
            pid: 1,
            cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            http: reqwest::Client::new(),
        });
        let req = Request::builder()
            .method("GET")
            .uri("/admin")
            .body(Body::empty())
            .expect("request");
        let res = app.oneshot(req).await.expect("response");
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), usize::MAX).await.expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("Codivex Admin"));
        assert!(text.contains("Project Indexing"));
    }
}
