use std::{collections::HashMap, path::Path};

use common::{CodeChunk, SearchCodeResult, SearchResultItem, projects};
use embeddings::{EmbeddingConfig, EmbeddingEngine};
use qdrant_client::Qdrant;
use search_core::{
    RetrievalDefaults,
    lexical::TantivyLexicalIndex,
    rrf_fuse,
    vector::{QdrantVectorStore, VectorSearchConfig},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetrievalTier {
    Fast,
    Hybrid,
    HybridRerank,
}

impl RetrievalTier {
    fn from_env() -> Self {
        match std::env::var("MCP_RETRIEVAL_TIER")
            .unwrap_or_else(|_| "hybrid".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "fast" => Self::Fast,
            "hybrid_rerank" | "hybrid-rerank" | "rerank" => Self::HybridRerank,
            _ => Self::Hybrid,
        }
    }
}

pub fn cache_key(project_scope: &str, query: &str, top_k: usize) -> String {
    format!("{project_scope}\u{241f}{query}\u{241f}{top_k}")
}

pub async fn cache_lookup(
    cache: &tokio::sync::Mutex<lru::LruCache<String, SearchCodeResult>>,
    key: &str,
) -> Option<SearchCodeResult> {
    let mut guard = cache.lock().await;
    guard.get(key).cloned()
}

pub async fn cache_store(
    cache: &tokio::sync::Mutex<lru::LruCache<String, SearchCodeResult>>,
    key: String,
    result: SearchCodeResult,
) {
    let mut guard = cache.lock().await;
    guard.put(key, result);
}

pub async fn scoped_project_results(
    cwd: &Path,
    project_path: &str,
    query: &str,
    top_k: usize,
) -> anyhow::Result<Vec<SearchResultItem>> {
    let indexed = projects::load_project_index(cwd, project_path)
        .ok_or_else(|| anyhow::anyhow!("project not indexed"))?;

    let project_chunks = indexed.chunks.iter().map(to_code_chunk).collect::<Vec<_>>();
    if project_chunks.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_map = project_chunks
        .iter()
        .map(|c| (c.id.clone(), c))
        .collect::<HashMap<_, _>>();

    let exact_symbol_hit = project_chunks
        .iter()
        .find(|c| {
            c.symbol
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(query.trim()))
        })
        .map(|c| c.id.clone());

    let defaults = RetrievalDefaults::default();
    let lexical_top_k = defaults.lexical_top_k.max(top_k.saturating_mul(4));
    let tier = RetrievalTier::from_env();

    let lexical_ids = lexical_ranked_ids(cwd, project_path, &project_chunks, query, lexical_top_k)?;
    let mut ordered_ids = Vec::new();
    if let Some(id) = exact_symbol_hit {
        ordered_ids.push(id);
    }

    match tier {
        RetrievalTier::Fast => {
            ordered_ids.extend(lexical_ids);
        }
        RetrievalTier::Hybrid | RetrievalTier::HybridRerank => {
            let semantic_ids =
                semantic_ranked_ids(project_path, &project_chunks, query, lexical_top_k).await;
            let fused = rrf_fuse(&lexical_ids, &semantic_ids, 60, 1.0, 0.7);
            ordered_ids.extend(fused.into_iter().map(|s| s.id));
        }
    }

    let mut dedup = HashMap::<String, ()>::new();
    let mut out = Vec::new();
    for id in ordered_ids {
        if dedup.contains_key(&id) {
            continue;
        }
        dedup.insert(id.clone(), ());
        if let Some(chunk) = chunk_map.get(&id) {
            out.push(SearchResultItem {
                file: chunk.file_path.clone(),
                function: chunk.symbol.clone().unwrap_or_else(|| "chunk".to_string()),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                code_block: trim_snippet(&chunk.content, 120, 6000),
            });
            if out.len() >= top_k.max(1) {
                break;
            }
        }
    }

    if tier == RetrievalTier::HybridRerank {
        out = rerank_results(query, out);
    }
    Ok(out)
}

fn lexical_ranked_ids(
    cwd: &Path,
    project_path: &str,
    chunks: &[CodeChunk],
    query: &str,
    top_k: usize,
) -> anyhow::Result<Vec<String>> {
    let on_disk_dir = projects::project_lexical_index_dir(cwd, project_path);
    if on_disk_dir.join("meta.json").exists() {
        match TantivyLexicalIndex::open_or_create_on_disk(&on_disk_dir) {
            Ok(index) => return Ok(index.search_ids(query, top_k).unwrap_or_default()),
            Err(err) => tracing::warn!(
                project = project_path,
                error = %err,
                "failed opening persisted lexical index, falling back to in-memory rebuild"
            ),
        }
    }

    let mut index = TantivyLexicalIndex::new_in_memory()?;
    for chunk in chunks {
        index.add_chunk(chunk)?;
    }
    index.commit()?;
    Ok(index.search_ids(query, top_k).unwrap_or_default())
}

async fn semantic_ranked_ids(
    project_path: &str,
    chunks: &[CodeChunk],
    query: &str,
    top_k: usize,
) -> Vec<String> {
    let engine = EmbeddingEngine::new(EmbeddingConfig::default());
    let query_vector = match engine.embed_batch(&[query.to_string()]) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(project = project_path, error = %err, "query embedding failed");
            return Vec::new();
        }
    };
    let Some(q) = query_vector.first() else {
        return Vec::new();
    };

    if let Some(client) = qdrant_client_from_env() {
        let mut cfg = VectorSearchConfig {
            collection: projects::project_vector_collection(project_path),
            ..VectorSearchConfig::default()
        };
        cfg.vector_dim = q.len();
        let store = QdrantVectorStore::new(cfg);
        match store.search_similar_ids(&client, q.clone(), top_k).await {
            Ok(ids) if !ids.is_empty() => return ids,
            Ok(_) => {}
            Err(err) => tracing::warn!(
                project = project_path,
                error = %err,
                "semantic qdrant lookup failed, falling back to local cosine"
            ),
        }
    }

    let texts = chunks.iter().map(|c| c.content.clone()).collect::<Vec<_>>();
    let vectors = match engine.embed_batch(&texts) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                project = project_path,
                error = %err,
                "chunk embedding failed for local semantic fallback"
            );
            return Vec::new();
        }
    };
    let mut scored = chunks
        .iter()
        .zip(vectors.iter())
        .map(|(chunk, vec)| (cosine_similarity(q, vec), chunk.id.clone()))
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored
        .into_iter()
        .take(top_k)
        .map(|(_, id)| id)
        .collect::<Vec<_>>()
}

fn qdrant_client_from_env() -> Option<Qdrant> {
    let url = std::env::var("QDRANT_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    Qdrant::from_url(&url).build().ok()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (av, bv) in a.iter().zip(b.iter()) {
        dot += av * bv;
        na += av * av;
        nb += bv * bv;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn to_code_chunk(chunk: &projects::IndexedChunk) -> CodeChunk {
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

fn chunk_stable_id(chunk: &projects::IndexedChunk) -> String {
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

fn trim_snippet(content: &str, max_lines: usize, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, line) in content.lines().enumerate() {
        if idx >= max_lines {
            out.push_str("\n... (truncated)");
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        if out.len() > max_chars {
            out.truncate(max_chars);
            out.push_str("\n... (truncated)");
            break;
        }
    }
    if out.is_empty() {
        content.chars().take(max_chars).collect()
    } else {
        out
    }
}

fn rerank_results(query: &str, items: Vec<SearchResultItem>) -> Vec<SearchResultItem> {
    if items.len() <= 1 {
        return items;
    }

    let top_n = std::env::var("MCP_RERANK_TOP_N")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let rerank_count = items.len().min(top_n);
    let engine = EmbeddingEngine::new(EmbeddingConfig::default());
    let query_vecs = match engine.embed_batch(&[query.to_string()]) {
        Ok(v) => v,
        Err(_) => return items,
    };
    let Some(query_vec) = query_vecs.first() else {
        return items;
    };

    let to_rank = items
        .iter()
        .take(rerank_count)
        .map(|i| i.code_block.clone())
        .collect::<Vec<_>>();
    let cand_vecs = match engine.embed_batch(&to_rank) {
        Ok(v) => v,
        Err(_) => return items,
    };

    let mut iter = items.into_iter();
    let head = iter.by_ref().take(rerank_count).collect::<Vec<_>>();
    let tail = iter.collect::<Vec<_>>();
    let mut scored = head
        .into_iter()
        .zip(cand_vecs)
        .map(|(item, vec)| (cosine_similarity(query_vec, &vec), item))
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut reranked = scored.into_iter().map(|(_, item)| item).collect::<Vec<_>>();
    reranked.extend(tail);
    reranked
}

#[cfg(test)]
mod tests {
    use common::SearchCodeResult;
    use lru::LruCache;
    use std::num::NonZeroUsize;
    use tokio::sync::Mutex;

    use super::{RetrievalTier, cache_key, cache_lookup, cache_store, cosine_similarity};

    #[tokio::test]
    async fn cache_roundtrip() {
        let cache = Mutex::new(LruCache::new(NonZeroUsize::new(8).expect("non-zero")));
        let key = cache_key("/tmp/project", "hello", 5);
        let payload = SearchCodeResult { items: Vec::new() };

        assert!(cache_lookup(&cache, &key).await.is_none());
        cache_store(&cache, key.clone(), payload.clone()).await;
        assert_eq!(cache_lookup(&cache, &key).await, Some(payload));
    }

    #[test]
    fn cosine_similarity_is_one_for_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn retrieval_tier_defaults_to_hybrid() {
        // SAFETY: test-scoped env mutation for this key.
        unsafe {
            std::env::remove_var("MCP_RETRIEVAL_TIER");
        }
        assert_eq!(RetrievalTier::from_env(), RetrievalTier::Hybrid);
    }
}
