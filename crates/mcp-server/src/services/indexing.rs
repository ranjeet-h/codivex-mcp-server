use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use common::{
    CodeChunk,
    projects::{self, IndexedChunk, IndexedProject},
};
use embeddings::{EmbeddingConfig, EmbeddingEngine};
use indexer::incremental::{ByteEdit, incremental_reparse};
use qdrant_client::Qdrant;
use search_core::{
    lexical::TantivyLexicalIndex,
    vector::{QdrantVectorStore, QuantizationMode as VectorQuantizationMode, VectorSearchConfig},
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use tree_sitter::Point;

use crate::state::AppState;

pub fn spawn_background_indexing(state: AppState) {
    tokio::spawn(async move {
        let active_watchers = Arc::new(RwLock::new(HashSet::<String>::new()));
        loop {
            if state.is_shutting_down() {
                break;
            }
            let projects_to_watch = discover_projects(&state.cwd);
            for project_path in projects_to_watch {
                if state.is_shutting_down() {
                    break;
                }
                if !Path::new(&project_path).exists() {
                    continue;
                }
                if mark_watcher_if_new(&active_watchers, &project_path).await {
                    spawn_project_watcher(state.clone(), active_watchers.clone(), project_path);
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

fn spawn_project_watcher(
    state: AppState,
    active_watchers: Arc<RwLock<HashSet<String>>>,
    project_path: String,
) {
    tokio::spawn(async move {
        if let Err(err) = run_project_watcher(state.clone(), &project_path).await {
            warn!(
                project = project_path,
                error = %err,
                "project watcher stopped"
            );
            state
                .indexing_runtime
                .mark_error(&project_path, err.to_string())
                .await;
        }
        state
            .indexing_runtime
            .mark_watcher_active(&project_path, false)
            .await;
        let mut guard = active_watchers.write().await;
        guard.remove(&project_path);
    });
}

async fn mark_watcher_if_new(
    active_watchers: &Arc<RwLock<HashSet<String>>>,
    project_path: &str,
) -> bool {
    let mut guard = active_watchers.write().await;
    if guard.contains(project_path) {
        false
    } else {
        guard.insert(project_path.to_string());
        true
    }
}

async fn run_project_watcher(state: AppState, project_path: &str) -> anyhow::Result<()> {
    let (watcher, mut rx) = indexer::watcher::FileWatcher::start(&[PathBuf::from(project_path)])?;
    let _keep_alive = watcher;
    state
        .indexing_runtime
        .mark_watcher_active(project_path, true)
        .await;
    info!(project = project_path, "started file watcher");

    let mut snapshots: HashMap<String, String> = HashMap::new();
    loop {
        if state.is_shutting_down() {
            break;
        }
        let maybe_event = tokio::select! {
            ev = rx.recv() => ev,
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                continue;
            }
        };
        let Some(event) = maybe_event else {
            break;
        };
        state
            .indexing_runtime
            .set_queue_depth(project_path, rx.len() as u64)
            .await;
        state.indexer_telemetry.set_queue_depth(rx.len() as u64);
        metrics::gauge!("index_queue_depth").set(rx.len() as f64);

        let mut touched_any = false;
        for path in event.paths {
            if !path.starts_with(project_path) {
                continue;
            }
            if path.is_dir() {
                continue;
            }
            touched_any = true;

            if let Ok(new_content) = std::fs::read_to_string(&path) {
                let key = path.to_string_lossy().to_string();
                if let Some(old_content) = snapshots.get(&key) {
                    let _ = try_incremental_parse(&key, old_content, &new_content);
                }
                snapshots.insert(key, new_content);
            }

            if let Err(err) = apply_incremental_update(&state, project_path, &path).await {
                warn!(
                    project = project_path,
                    file = %path.display(),
                    error = %err,
                    "incremental update failed"
                );
                state
                    .indexing_runtime
                    .mark_error(project_path, err.to_string())
                    .await;
            }
        }

        if touched_any {
            metrics::counter!("index_updates_total").increment(1);
        }
    }

    Ok(())
}

async fn apply_incremental_update(
    state: &AppState,
    project_path: &str,
    changed_path: &Path,
) -> anyhow::Result<()> {
    let cwd = state.cwd.clone();
    let project = project_path.to_string();
    let changed = changed_path.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        update_json_and_lexical_index(&cwd, &project, &changed)
    })
    .await??;

    if let Some(client) = qdrant_client_from_env() {
        let mut cfg = VectorSearchConfig {
            collection: projects::project_vector_collection(project_path),
            ..VectorSearchConfig::default()
        };
        if let Some(first) = output.added_chunks.first() {
            let embedding_cfg = EmbeddingConfig::default();
            let engine = EmbeddingEngine::new(embedding_cfg.clone());
            let texts = output
                .added_chunks
                .iter()
                .map(|c| c.content.clone())
                .collect::<Vec<_>>();
            let vectors = engine.embed_batch(&texts)?;
            if let Some(first_vec) = vectors.first() {
                cfg.vector_dim = first_vec.len();
                cfg.quantization = to_vector_quantization_mode(embedding_cfg.quantization);
                let store = QdrantVectorStore::new(cfg.clone());
                let _ = store.ensure_collection(&client).await;
                if !output.deleted_chunk_ids.is_empty() {
                    let _ = store
                        .delete_points(&client, &output.deleted_chunk_ids)
                        .await;
                }
                store
                    .upsert_chunks(&client, &output.added_chunks, &vectors)
                    .await?;
            } else {
                let _ = first;
            }
        } else if !output.deleted_chunk_ids.is_empty() {
            let store = QdrantVectorStore::new(cfg);
            let _ = store
                .delete_points(&client, &output.deleted_chunk_ids)
                .await;
        }
    }

    let now_ms = unix_now_ms();
    state
        .indexing_runtime
        .mark_indexed(project_path, output.added_chunks.len() as u64)
        .await;
    state
        .indexing_runtime
        .set_queue_depth(project_path, 0)
        .await;
    state
        .indexer_telemetry
        .inc_chunks_indexed(output.added_chunks.len() as u64);
    state.indexer_telemetry.set_last_index_unix_ms(now_ms);
    metrics::gauge!("indexing_lag_ms").set(output.indexing_lag_ms as f64);
    metrics::gauge!("index_queue_depth").set(0.0);
    metrics::counter!("index_chunks_added_total").increment(output.added_chunks.len() as u64);
    debug!(
        project = project_path,
        file = %changed_path.display(),
        added = output.added_chunks.len(),
        deleted = output.deleted_chunk_ids.len(),
        "incremental index update complete"
    );
    Ok(())
}

#[derive(Debug)]
struct IncrementalUpdateOutput {
    added_chunks: Vec<CodeChunk>,
    deleted_chunk_ids: Vec<String>,
    indexing_lag_ms: u64,
}

fn update_json_and_lexical_index(
    cwd: &Path,
    project_path: &str,
    changed_path: &Path,
) -> anyhow::Result<IncrementalUpdateOutput> {
    let mut indexed = projects::load_project_index(cwd, project_path).ok_or_else(|| {
        anyhow::anyhow!("project not indexed yet: {project_path}, run initial indexing first")
    })?;

    let changed_path_str = changed_path.to_string_lossy().to_string();
    let mut deleted_chunk_ids = Vec::new();
    indexed.chunks.retain(|chunk| {
        let keep = !same_file(project_path, &changed_path_str, &chunk.file);
        if !keep {
            deleted_chunk_ids.push(chunk_stable_id(chunk));
        }
        keep
    });

    let mut added_chunks = Vec::new();
    if changed_path.exists() {
        if let Ok(content) = std::fs::read_to_string(changed_path) {
            if let Ok(chunks) = indexer::extract_chunks_for_file(&changed_path_str, &content) {
                for chunk in chunks {
                    indexed.chunks.push(IndexedChunk {
                        file: chunk.file_path.clone(),
                        symbol: chunk.symbol.clone(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        content: chunk.content.clone(),
                    });
                    added_chunks.push(chunk);
                }
            }
        }
    }

    indexed.chunks_extracted = indexed.chunks.len();
    indexed.indexed_at_unix = unix_now();
    projects::save_project_index(cwd, &indexed)?;

    persist_tantivy_index(cwd, project_path, &indexed)?;

    let lag_ms = 0u64;
    Ok(IncrementalUpdateOutput {
        added_chunks,
        deleted_chunk_ids,
        indexing_lag_ms: lag_ms,
    })
}

fn persist_tantivy_index(
    cwd: &Path,
    project_path: &str,
    indexed: &IndexedProject,
) -> anyhow::Result<()> {
    let chunks = indexed.chunks.iter().map(to_code_chunk).collect::<Vec<_>>();
    let index_dir = projects::project_lexical_index_dir(cwd, project_path);
    let mut index = TantivyLexicalIndex::open_or_create_on_disk(&index_dir)?;
    index.reset()?;
    for chunk in &chunks {
        index.add_chunk(chunk)?;
    }
    index.commit()?;
    Ok(())
}

fn to_code_chunk(chunk: &IndexedChunk) -> CodeChunk {
    CodeChunk {
        id: chunk_stable_id(chunk),
        fingerprint: chunk_stable_id(chunk),
        file_path: chunk.file.clone(),
        language: language_from_path(&chunk.file),
        symbol: chunk.symbol.clone(),
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        start_char: 0,
        end_char: chunk.content.len(),
        content: chunk.content.clone(),
    }
}

fn chunk_stable_id(chunk: &IndexedChunk) -> String {
    format!(
        "{}:{}:{}:{}",
        chunk.file,
        chunk.start_line,
        chunk.end_line,
        chunk.symbol.clone().unwrap_or_default()
    )
}

fn language_from_path(path: &str) -> String {
    if path.ends_with(".rs") {
        "rust".to_string()
    } else if path.ends_with(".c") || path.ends_with(".h") {
        "c".to_string()
    } else if path.ends_with(".cc")
        || path.ends_with(".cpp")
        || path.ends_with(".cxx")
        || path.ends_with(".hpp")
        || path.ends_with(".hh")
        || path.ends_with(".hxx")
        || path.ends_with(".ipp")
        || path.ends_with(".tpp")
        || path.ends_with(".inl")
    {
        "cpp".to_string()
    } else if path.ends_with(".ts") || path.ends_with(".tsx") {
        "typescript".to_string()
    } else if path.ends_with(".js") || path.ends_with(".jsx") {
        "javascript".to_string()
    } else if path.ends_with(".py") || path.ends_with(".pyi") {
        "python".to_string()
    } else if path.ends_with(".go") {
        "go".to_string()
    } else if path.ends_with(".hs") || path.ends_with(".lhs") {
        "haskell".to_string()
    } else if path.ends_with(".java") {
        "java".to_string()
    } else if path.ends_with(".cs") {
        "csharp".to_string()
    } else if path.ends_with(".php") || path.ends_with(".phtml") {
        "php".to_string()
    } else if path.ends_with(".rb") {
        "ruby".to_string()
    } else if path.ends_with(".kt") || path.ends_with(".kts") {
        "kotlin".to_string()
    } else if path.ends_with(".swift") {
        "swift".to_string()
    } else {
        "text".to_string()
    }
}

fn same_file(project_path: &str, changed_path: &str, chunk_file: &str) -> bool {
    let changed = normalize_path(changed_path);
    let chunk = if Path::new(chunk_file).is_absolute() {
        normalize_path(chunk_file)
    } else {
        normalize_path(&Path::new(project_path).join(chunk_file).to_string_lossy())
    };
    changed == chunk
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn discover_projects(cwd: &Path) -> Vec<String> {
    let mut projects = projects::read_catalog(cwd)
        .projects
        .into_iter()
        .map(|entry| entry.project_path)
        .collect::<Vec<_>>();
    if let Some(selected) = projects::read_selected_project(cwd) {
        projects.push(selected);
    }
    projects.sort();
    projects.dedup();
    projects
}

fn qdrant_client_from_env() -> Option<Qdrant> {
    let url = std::env::var("QDRANT_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    Qdrant::from_url(&url).build().ok()
}

fn to_vector_quantization_mode(mode: embeddings::QuantizationMode) -> VectorQuantizationMode {
    match mode {
        embeddings::QuantizationMode::None => VectorQuantizationMode::None,
        embeddings::QuantizationMode::Int8 => VectorQuantizationMode::Int8,
        embeddings::QuantizationMode::UInt8 => VectorQuantizationMode::UInt8,
    }
}

fn try_incremental_parse(path: &str, old_source: &str, new_source: &str) -> bool {
    if old_source == new_source {
        return true;
    }
    let Some((start_byte, old_end_byte, new_end_byte)) = compute_edit_span(old_source, new_source)
    else {
        return false;
    };
    let edit = ByteEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: byte_to_point(old_source, start_byte),
        old_end_position: byte_to_point(old_source, old_end_byte),
        new_end_position: byte_to_point(new_source, new_end_byte),
    };
    incremental_reparse(path, old_source, new_source, edit).is_ok()
}

fn compute_edit_span(old_source: &str, new_source: &str) -> Option<(usize, usize, usize)> {
    let old_bytes = old_source.as_bytes();
    let new_bytes = new_source.as_bytes();
    if old_bytes == new_bytes {
        return None;
    }

    let mut prefix = 0usize;
    let min_len = old_bytes.len().min(new_bytes.len());
    while prefix < min_len && old_bytes[prefix] == new_bytes[prefix] {
        prefix += 1;
    }

    let mut old_suffix = old_bytes.len();
    let mut new_suffix = new_bytes.len();
    while old_suffix > prefix
        && new_suffix > prefix
        && old_bytes[old_suffix - 1] == new_bytes[new_suffix - 1]
    {
        old_suffix -= 1;
        new_suffix -= 1;
    }

    Some((prefix, old_suffix, new_suffix))
}

fn byte_to_point(source: &str, byte_index: usize) -> Point {
    let mut row = 0usize;
    let mut col = 0usize;
    let clamped = byte_index.min(source.len());
    for &b in source.as_bytes().iter().take(clamped) {
        if b == b'\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Point { row, column: col }
}

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

fn unix_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as u64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{byte_to_point, compute_edit_span, normalize_path};

    #[test]
    fn compute_edit_span_detects_middle_change() {
        let span = compute_edit_span("abcde", "abXYde").expect("span");
        assert_eq!(span, (2, 3, 4));
    }

    #[test]
    fn byte_to_point_tracks_rows_and_columns() {
        let p = byte_to_point("a\nbc\n", 4);
        assert_eq!(p.row, 1);
        assert_eq!(p.column, 2);
    }

    #[test]
    fn normalize_path_unifies_windows_style_paths() {
        assert_eq!(normalize_path("a\\b\\c.rs"), "a/b/c.rs");
    }
}
