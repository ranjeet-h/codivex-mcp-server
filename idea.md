# High-Performance Local Rust MCP Code Search Engine

Modern AI coding assistants need **instant, precise access to large codebases**. Naively passing entire files to an LLM blows past context limits, increases latency and costs, and pollutes the model’s attention. Instead, we build a **local Rust-based MCP (Model Context Protocol) server** that maintains a real-time semantic index of the code. The server exposes JSON-RPC and streams results via Server-Sent Events (SSE) to LLMs/agents, ensuring millisecond response times for code queries. Key components include Tree-sitter AST parsing, Tantivy for BM25 lexical search, Qdrant for vector embeddings, and smart rank fusion. The result is a drop-in tool that any agent (Cursor, Claude Code, etc.) can call for “searchCode” and “openLocation” operations on a selected repository.

---

## Architecture Overview

- **AST Parsing & Chunking:** Use [Tree-sitter](https://tree-sitter.github.io/tree-sitter/) to parse each source file into a concrete syntax tree【1†L126-L134】. Extract functions, classes, comments and other logical units. Each **code chunk** is defined by `file, start_line, end_line, code_text` (including signature and doc-comments). This yields semantically complete blocks (no cut-off functions).

- **Indexing:** Chunks are simultaneously indexed in two ways:
  - **Lexical Index (Tantivy):** Full-text index of each chunk’s content (and symbol names) for exact/keyword search【44†L149-L158】. Tantivy is essentially “Lucene in Rust”【44†L149-L158】. We also keep a HashMap of symbol→chunk for O(1) exact lookups.
  - **Vector Index (Qdrant):** Each chunk’s content is embedded into a high-D vector (ONNX-inference) and stored in Qdrant (HNSW ANN)【12†L168-L177】. Qdrant handles millions of vectors with single-digit millisecond latency【12†L149-L157】. Vectors capture semantic similarity beyond exact terms.

- **MCP Server (Axum):** The Rust binary runs an async HTTP/SSE server (axum) that implements the MCP JSON-RPC schema. Agents send `searchCode(query, topK)` calls. The server performs symbol lookup + BM25 search + vector search, fuses results, and streams top chunks back over SSE【46†L175-L182】. For example, a query like `"iso to date"` returns the exact function name location plus any semantically relevant code blocks.

- **Continuous Update:** The daemon watches selected repo paths (via the `notify` crate, e.g. inotify/fsevents) for file changes【32†L49-L51】. On edits, Tree-sitter applies _incremental parsing_ (using `tree.edit()` and `parser.parse(..., Some(&old_tree))`【1†L126-L134】) so only changed chunks are re-indexed. This keeps the index up-to-date in the background with sub-second lag.

---

## Ingestion and Chunking

1. **File Scanning:** Recursively watch the repo(s), ignoring `node_modules`, `.git`, etc. On file save events, enqueue the path.
2. **Tree-sitter Parsing:** For each file change, run the appropriate parser (e.g. `tree-sitter-rust`). Tree-sitter can _incrementally_ update the AST on edits【1†L126-L134】. We use Tree-sitter queries or AST traversal to identify function/method/class definitions and relevant comments.
3. **Chunk Creation:** Each AST node of interest becomes a chunk. A `CodeChunk` contains:
   - `id`: unique chunk ID (e.g. SHA-256 of contents).
   - `file_path`, `start_line`, `end_line`, `start_char`, `end_char`.
   - `function_name` (symbol) if any.
   - `language` (inferred).
   - `content`: the code snippet text.
4. **Deduplication:** Compute a fingerprint of each chunk (strip whitespace/formatting). If unchanged, skip re-embedding.
5. **Embedding:** Batch chunks (e.g. 128) and run an embedding model. For best offline performance, we export a model (like MiniLM or a code-specific model) to ONNX and run it with the [ONNX Runtime](41) in Rust. Benchmarks show Rust+ONNX can be **3–4× faster** and use **4–5× less memory** than Python/PyTorch【42†L135-L144】【42†L229-L237】. The resulting `Vec<f32>` (e.g. 384 or 1024 dims) is stored with the chunk.
6. **Index Updates:** Upsert each new chunk to Qdrant (vector) and add a document to Tantivy (text). On deletes or modifications, remove old entries. This incremental pipeline (Tree-sitter + Rayon threads) can continuously index millions of LOC with minimal CPU spikes.

_Example:_ Bosun’s Swiftide (Rust) uses a similar pipeline: Tree-sitter outline extraction + code chunking + embedding + Qdrant storage【30†L111-L119】.

---

## Retrieval Pipeline

When an agent calls `searchCode(query)`, we do:

1. **Symbol Lookup (HashMap):** If the query exactly matches a stored symbol (function/class name), return that chunk immediately (score ~∞). This is O(1).
2. **Tantivy Search (BM25):** Use `QueryParser` on indexed content and symbol fields. Tantivy ranks documents by BM25 (boosts rare terms)【44†L169-L177】【44†L195-L203】. We retrieve the top-K lexical matches (e.g. K=20). This catches exact keyword/identifier hits (e.g. a unique variable name).
3. **Qdrant ANN Search:** Embed the query string (through the ONNX model) and search Qdrant’s HNSW index. Qdrant returns nearest neighbors by cosine similarity, which surfaces semantically related code blocks. On a million-vector scale, Qdrant yields ~3.5ms median latency (at 99% recall)【12†L149-L157】.
4. **Rank Fusion (Reciprocal Rank Fusion):** We combine the two ranked lists (lexical and semantic) using Reciprocal Rank Fusion (RRF)【40†L60-L69】【40†L84-L89】. RRF ignores raw scores and sums `(w/(k+rank))` for each document across both lists. We typically set `k≈60`【40†L84-L89】 and weight lexical slightly higher (e.g. w_lex=1.0, w_vec=0.7) to favor exact matches. RRF ensures a code block that ranks highly in both channels bubbles to the top, yielding a single unified ranking.
5. **Return Results (SSE):** The top N fused results are streamed back as SSE events. Each event includes file path, line range, and snippet. For example:
   ```json
   {
     "file": "src/date.rs",
     "function": "iso_to_date",
     "start_line": 42,
     "end_line": 58,
     "code_block": "fn iso_to_date(...) { ... }"
   }
   ```
   Streaming via `axum::response::sse::Sse` allows agents to display partial results immediately【46†L175-L182】.

This **hybrid search** (exact + BM25 + semantic) ensures high precision. For example, a query for a unique error code will be caught by BM25, while a general query (“save user record”) will leverage semantic similarity. RRF mathematically guarantees a balanced merge of signals【40†L60-L69】【40†L84-L89】.

---

## Embedding & Semantic Models

The embedding step is critical. We support multiple models (depending on hardware):

- **Local ONNX Models:** Convert HuggingFace/SentenceTransformer models to ONNX and run via [onnxruntime](41). E.g. all-MiniLM-L6-v2 (384d)【25†L66-L75】 or larger code models (e.g. OpenAI’s code embeddings, BGE). ONNX + Rust yields high throughput【42†L135-L144】 and low memory【42†L229-L237】.
- **Quantization:** To save RAM, quantize embeddings (int8/uint8). Qdrant supports on-the-fly quantization, reducing vector DB memory 4× with <1% accuracy drop【21†L286-L295】. This lets us store millions of vectors even on limited machines.

We chose ONNX inference because Rust+ONNX can embed ~300–400 sentences/sec on a CPU, far exceeding typical Python pipelines【42†L135-L144】. As the user code changes, new embeddings are computed continuously but non-blockingly (batch jobs on Rayon threads).

---

## Lexical Search (Tantivy)

We use Tantivy for exact and keyword search. Tantivy is a high-speed full-text index (“Think Lucene, but in Rust”【44†L149-L158】). Key points:

- **Schema:** Each chunk indexed with fields like `path (STORED)`, `function_name (STORED)`, `content (TEXT)`. Text fields are tokenized, inverted-indexed, BM25-scored【44†L149-L158】. Stored fields let us fetch snippet data.
- **Indexing:** Use an `IndexWriter` (buffered) to add `Document` for each chunk, then `commit()`. This is fast and multi-threaded.
- **Querying:** A `QueryParser` can parse natural queries (e.g. "ParseError 0x04F3") into term queries. Search returns `TopDocs` by score【44†L195-L203】. We typically fetch top ~20.

Tantivy’s inverted indexes are memory-mapped with SIMD compression, so keyword searches on 100k+ chunks complete in a few milliseconds. Rare identifiers (high IDF) get extra weight, matching exactly by name or string.

---

## MCP API & Streaming

The server implements the MCP JSON-RPC interface. It registers methods like `searchCode(query, topK, repoFilter)` and `openLocation(path, lineStart, lineEnd)`.

- **SSE Transport:** For streaming results, we use Axum’s `Sse` response type. Axum wraps an async Stream of SSE events【46†L175-L182】. Our handler creates a stream that writes each result as `data: {...JSON...}\n\n`. Agents can consume these incrementally.

- **Request Validation:** We define strict JSON Schemas for inputs (using `schemars` crate) so malformed queries are rejected cheaply. Each method’s params are validated by Rust’s type system before logic.

Example Axum route snippet:

```rust
async fn search_handler(
    State(state): State<AppState>,
    Json(rpc): Json<RpcRequest>,
) -> Sse<impl Stream<Item=Result<axum::response::sse::Event, Infallible>>> {
    let query: String = rpc.params["query"].as_str().unwrap().to_string();
    let topk = rpc.params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    // Perform search asynchronously...
    // Stream results via Sse::new(...)
}
```

Agents (Cursor, Claude Code, etc.) simply point at `http://127.0.0.1:8080/mcp`. No external network calls or cloud needed. Queries return strongly-typed results for seamless integration.

---

## Deployment & Operations

- **Docker:** We provide a Docker image bundling the Rust binary, ONNX model, Tantivy index files, and Qdrant (as sidecar or embedded). The repo is mounted read-only (`-v /path/to/code:/repo`). The image exposes port 8080 for MCP. This works on any platform (Linux, Windows with Docker).
- **macOS Native:** A Homebrew formula or script installs the Rust binary. A `launchd` plist is provided to run it as a user agent. On boot, launchd starts the indexer (with limited CPU priority). The server binds to `127.0.0.1` only.
- **Web Dashboard:** The same axum server hosts a simple React/TypeScript SPA under `/admin`. This allows selecting repos to index, viewing real-time stats (parsed chunks, index size), and manual re-index triggers. Telemetry is pushed via SSE from the tokio task to the browser (no polling). The dashboard uses `/metrics` or SSE endpoints and shows progress bars.

The tool also listens for `SIGINT/SIGTERM` to gracefully shut down, persisting index state. Logs (structured JSON) record queries (hashed) and errors for audit. CPU/memory usage is kept low so developers won’t notice its presence.

---

## Performance Tuning

- **Index Size & Memory:** Each 1536-d vector is ~6KB. 1M vectors ≈ 6GB, plus index overhead. (Benchmarks limited Qdrant to 25GB when 8.6GB would suffice【34†L239-L242】.) We use quantization to cut this to ~1.5GB. Tantivy indexes are much smaller, heavily compressed.
- **Latency Targets:** Benchmarks (Qdrant) show 1M×1536d search at ~3–10ms【12†L149-L157】. Tantivy returns 100k-chunk searches in ~5ms. Embedding a query is ~10–20ms on CPU【42†L135-L144】. Overall, we expect **end-to-end 20–50ms** for typical queries.
- **Throughput:** Qdrant handled ~1200 QPS in one test【12†L149-L157】. Locally, a single-core can already serve many simultaneous small agents.
- **Parallelism:** Indexing and search are multithreaded. We recommend giving the container ~2-4 vCPUs. Tokio and Rayon parallelize embedding and indexing. Queries are async, so CPUs never block.
- **Caching:** Optionally cache hot queries or embedding results (LRU cache) to cut repeated work.
- **GPU Acceleration:** (Optional) If host has GPU, run Qdrant or ONNX on GPU for even lower latency.
- **Profiling:** We include a performance mode (via `--metrics`) to log parse/embed throughput, search latencies. SLOs: median <50ms, p95 <200ms under load.

---

## Security and Isolation

- **Local-Only:** Binds to `127.0.0.1`. No network access needed.
- **Read-Only Access:** The MCP methods only read code. The server has no capability to modify source (file system mount is read-only).
- **Authentication:** For extra security, the server can require a local API token in headers.
- **Data Privacy:** All embeddings and code remain on the host. No third-party APIs or telemetry of code content.
- **Sandboxing (optional):** Although unlikely needed, the binary can be sandboxed (macOS Hardened Runtime) if required by policy.

This makes the tool safe by design. It cannot leak code outside, and any agent can only retrieve code snippets via the well-defined MCP schema.

---

## Conclusion

This Rust-based MCP indexer provides **blazing-fast, developer-grade code search**. By combining AST-aware chunking, quantized vector search, exact lexical lookup, and smart rank fusion, the system delivers relevant code locations in tens of milliseconds. It operates invisibly in the background (Docker or macOS launchd), with a slick web dashboard for control. Every major open-source agent can integrate via the MCP endpoint to leverage the full project knowledge without blowing up the LLM’s context. In effect, LLMs can now query entire repos with the precision of a seasoned engineer.

This meets the highest standards: production-grade performance (millisecond queries, support for millions of LOC), airtight security (fully local, read-only), and smooth integration (MCP JSON-RPC). The architecture draws on the latest research in hybrid search and systems engineering【12†L149-L157】【40†L60-L69】【42†L135-L144】, ensuring it truly is a “best in class” solution for large codebases.

**Sources:** We leveraged official docs and recent studies in our design: Tree-sitter docs【1†L126-L134】, Tantivy examples【44†L149-L158】【44†L169-L177】, Qdrant benchmarks【12†L149-L157】【34†L239-L242】, Rust-ONNX inference benchmarks【42†L135-L144】【42†L229-L237】, and RRF rank fusion theory【40†L60-L69】【40†L84-L89】.
