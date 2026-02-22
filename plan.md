# Implementation Plan: High-Performance Local Rust MCP Code Search Engine

## Checklist Status Legend
- `[ ]` Not started
- `[-]` In progress
- `[x]` Completed

## Phase 0: Foundations and Scope Lock
- [ ] Confirm product scope from `idea.md` and freeze initial v1 requirements (indexing, hybrid search, MCP API, SSE streaming, Dioxus admin UI, metrics, tests).
- [ ] Define non-goals for v1 (for example: multi-tenant auth, distributed clustering, GPU-first deployment) to keep delivery focused.
- [ ] Define acceptance criteria with measurable targets:
  - Median query latency < 50ms
  - p95 query latency < 200ms
  - Incremental re-index delay < 1s for small edits
  - Manual test coverage for end-to-end UX in Dioxus UI
- [ ] Create architecture decision records (ADRs) for core choices: Tree-sitter chunking, Tantivy BM25, Qdrant ANN, RRF fusion, Axum+SSE transport.

## Phase 1: Project Setup and Latest Rust Dependencies
- [ ] Initialize workspace structure:
  - `crates/mcp-server`
  - `crates/indexer`
  - `crates/search-core`
  - `crates/embeddings`
  - `crates/ui-dioxus`
  - `crates/common`
- [ ] Pin toolchain to stable Rust (latest stable channel at implementation time) and configure:
  - `rust-toolchain.toml`
  - `cargo fmt`
  - `clippy` (deny warnings for CI)
  - `cargo nextest`
- [ ] Add latest crate versions (at implementation time), then lock with `Cargo.lock`:
  - Runtime/API: `tokio`, `axum`, `tower`, `tower-http`, `serde`, `serde_json`, `schemars`, `tracing`, `tracing-subscriber`
  - Parsing/indexing: `tree-sitter`, language grammars, `tantivy`, `notify`, `ignore`, `rayon`
  - Vector DB and embeddings: `qdrant-client`, `ort` or `onnxruntime`, `tokenizers`
  - Utility/perf: `ahash`, `dashmap`, `parking_lot`, `lru`, `metrics`, `metrics-exporter-prometheus`
  - UI: `dioxus` (latest), `dioxus-router`, `dioxus-logger`
  - Testing/bench: `criterion`, `insta`, `reqwest`, `proptest`
- [ ] Add dependency hygiene gates:
  - `cargo audit`
  - `cargo deny`
  - `cargo outdated` (informational)

## Phase 1.1: Idea.md Tech Coverage Matrix (Line-by-Line)
- [ ] Verify and keep this matrix complete against `idea.md` before implementation starts.
- [ ] Core protocol and transport from `idea.md`:
  - MCP (Model Context Protocol) server
  - JSON-RPC request/response contract
  - SSE streaming responses
  - Axum async HTTP server
- [ ] Parsing and indexing stack from `idea.md`:
  - Tree-sitter AST parsing
  - Tree-sitter incremental edits (`tree.edit` + incremental reparse)
  - Tantivy lexical search (BM25)
  - Symbol `HashMap` for O(1) exact lookup
  - Qdrant vector store
  - HNSW ANN index usage
  - Cosine similarity retrieval
  - Reciprocal Rank Fusion (RRF) with tunable `k` and weights
- [ ] Change detection and background processing from `idea.md`:
  - `notify` watcher integration
  - OS backend expectations (`inotify`/`fsevents` behavior)
  - Tokio async runtime
  - Rayon for parallel indexing/embedding jobs
- [ ] Embedding runtime from `idea.md`:
  - ONNX model support
  - Rust ONNX runtime (`ort`/`onnxruntime`)
  - Batch embedding queue
  - Optional quantization strategy (int8/uint8)
- [ ] API and schema safety from `idea.md`:
  - `searchCode(query, topK, repoFilter?)`
  - `openLocation(path, lineStart, lineEnd)`
  - `schemars`-based schema validation
- [ ] Operations and deployment from `idea.md`:
  - Docker image path (Rust service + ONNX model + Tantivy index + Qdrant sidecar/embedded)
  - Read-only repo mount behavior
  - macOS native install path (script/Homebrew compatible)
  - `launchd` user agent startup
  - `SIGINT`/`SIGTERM` graceful shutdown path
- [ ] Observability and security from `idea.md`:
  - Structured JSON logs
  - Query hash logging (no raw query persistence by default)
  - `/metrics` endpoint and/or SSE telemetry
  - Local-only bind (`127.0.0.1`)
  - Optional local API token auth
  - Optional macOS Hardened Runtime sandboxing
- [ ] Dashboard/UI alignment decision from `idea.md`:
  - Replace React/TypeScript `/admin` dashboard with Rust Dioxus UI
  - Preserve all functional parity from idea dashboard requirements
  - Keep `/admin` route semantics by serving Dioxus UI at `/admin`

## Phase 2: Configuration and Unique Port Allocation
- [ ] Design config model (`config.toml` + env override) covering repo paths, ignored paths, model path, topK defaults, telemetry flags, auth token.
- [ ] Implement local-only binding policy (`127.0.0.1` by default) and explicit opt-in for non-local interfaces.
- [ ] Implement collision-safe unique port allocation for MCP server and Dioxus UI:
  - Prefer configured port when free.
  - If occupied, probe deterministic fallback range (example: `38080..38180`) for first free port.
  - Persist chosen ports in runtime state file (example: `.codivex/runtime-ports.json`) to keep stable between restarts.
  - Print active endpoints clearly in startup logs.
- [ ] Add startup port diagnostics endpoint in UI (shows bound ports, conflicts resolved, process PID).
- [ ] Reserve and expose unique ports for:
  - MCP JSON-RPC/SSE server
  - Dioxus `/admin` UI
  - Optional `/metrics` endpoint when split is enabled
- [ ] Add tests for port allocator behavior (configured-free, configured-busy, all-busy, restart-stability).

## Phase 3: File Discovery, AST Parsing, and Chunking
- [ ] Implement recursive repository scanner with ignore rules (`.git`, `node_modules`, target/build artifacts, large binaries).
- [ ] Build language detection + parser registry for Tree-sitter grammars needed for v1.
- [ ] Implement chunk extraction strategy:
  - Function/method/class blocks
  - Signature + doc-comments
  - File and positional metadata
- [ ] Define canonical `CodeChunk` schema:
  - `id`, `fingerprint`, `file_path`, `language`, `symbol`, `start_line`, `end_line`, `start_char`, `end_char`, `content`
- [ ] Implement deterministic chunk fingerprinting (whitespace-normalized hash) and dedup skip logic.
- [ ] Add incremental parse pipeline (`tree.edit` + parse with previous tree) for changed files.
- [ ] Create chunking quality tests using fixtures across multiple languages.

## Phase 4: Indexing Pipeline (Lexical + Vector)
- [ ] Implement Tantivy schema and indexing:
  - Indexed fields: content, symbol, path
  - Stored fields: file metadata/snippet
- [ ] Implement Qdrant collection lifecycle:
  - Collection creation/validation
  - Vector dimension checks
  - Distance metric explicitly set to cosine
  - HNSW params baseline for local low-latency profile
  - Optional quantization configuration
  - Upsert and delete paths
- [ ] Implement embedding engine with batch worker pool:
  - Model load once
  - Batch queue (configurable size)
  - Backpressure + retry policy
- [ ] Implement incremental synchronization logic:
  - New chunks -> upsert both indexes
  - Modified chunks -> replace previous entries
  - Deleted chunks/files -> remove stale entries
- [ ] Implement health/status telemetry:
  - Queue depth
  - Chunks indexed
  - Last index timestamp
  - Embedding throughput

## Phase 5: Retrieval and Rank Fusion
- [ ] Implement O(1) symbol lookup map (exact symbol hit shortcut).
- [ ] Implement BM25 query flow using Tantivy `QueryParser`.
- [ ] Implement vector query flow:
  - Embed query text
  - Search Qdrant ANN
- [ ] Implement Reciprocal Rank Fusion:
  - Tunable `k` (default 60)
  - Tunable weights (`w_lex`, `w_vec`)
- [ ] Define unified result DTO with score explainability fields:
  - lexical rank/score
  - vector rank/score
  - fused score
- [ ] Add relevance evaluation harness with known query→expected-file fixtures.

## Phase 6: MCP JSON-RPC + SSE Transport
- [ ] Define method contracts and schema validation:
  - `searchCode(query, topK, repoFilter?)`
  - `openLocation(path, lineStart, lineEnd)`
- [ ] Implement Axum routes and JSON-RPC dispatcher.
- [ ] Implement SSE streaming for partial results and completion events.
- [ ] Add robust error model (validation errors, index unavailable, timeout, internal failure).
- [ ] Add request tracing with correlation IDs and latency timing.
- [ ] Add strict JSON schema checks (`schemars`) before method execution.
- [ ] Add integration tests for RPC methods and SSE stream order/format.

## Phase 7: Dioxus Rust Admin UI (Manual Testing Console)
- [ ] Build Dioxus UI served by Rust backend on its unique allocated port.
- [ ] Serve Dioxus UI under `/admin` route for compatibility with original idea contract.
- [ ] Implement key screens:
  - Repo selection and indexing controls
  - Search playground with live SSE stream
  - Result list with file path + line-range + code preview
  - System health (ports, queue depth, index sizes, latency stats)
- [ ] Add manual-test-first UX controls:
  - Input presets for common queries
  - One-click “Run Smoke Test”
  - Visual pass/fail checklist panel
- [ ] Add explicit UI display for active MCP endpoint and UI endpoint to avoid localhost port confusion.
- [ ] Add UI actions to trigger re-index and clear/rebuild index with confirmations.

## Phase 8: Performance and Speed Testing
- [ ] Create reproducible benchmark dataset profiles:
  - Small repo (~50k LOC)
  - Medium repo (~500k LOC)
  - Large repo (1M+ LOC)
- [ ] Implement benchmark command suite:
  - Cold start indexing time
  - Incremental update time per file edit
  - Query latency (p50, p95, p99)
  - Throughput under concurrency
- [ ] Add Criterion microbenchmarks for hot paths:
  - chunk extraction
  - embedding throughput
  - fusion logic
- [ ] Add load test runner for API/SSE.
- [ ] Expose benchmark summaries in Dioxus UI dashboard for manual verification.

## Phase 9: Manual Testing Plan via Dioxus UI
- [ ] Write manual QA checklist in repo (`manual-test-checklist.md`) using `[ ]`/`[-]`/`[x]` states.
- [ ] Run full manual flow from Dioxus UI:
  - Add repository and initial index
  - Execute exact-symbol query
  - Execute semantic query
  - Validate streamed partial results
  - Validate openLocation response integrity
  - Edit a file and validate incremental re-index behavior
- [ ] Validate edge cases:
  - Empty query
  - Oversized query
  - Unsupported file type
  - Qdrant unavailable
  - Port conflict at startup
- [ ] Capture issues with severity, reproduction steps, and fix owner.

## Phase 10: Security, Reliability, and Operations
- [ ] Enforce local-only defaults and optional token auth.
- [ ] Confirm read-only repository mount behavior in container mode.
- [ ] Add graceful shutdown (flush queues, close streams, persist runtime metadata).
- [ ] Add `SIGINT`/`SIGTERM` handling for graceful stop and state persistence.
- [ ] Add structured JSON logging with hashed query values and error audit trails.
- [ ] Add optional Prometheus metrics endpoint and SSE telemetry channel.
- [ ] Prepare Docker deployment and macOS launch agent scripts.
- [ ] Add Docker runtime modes:
  - Qdrant as sidecar
  - Qdrant embedded mode (if selected)
- [ ] Add macOS hardening option documentation (Hardened Runtime optional path).
- [ ] Add backup/rebuild strategy for indexes and corruption recovery flow.

## Phase 11: CI/CD and Release Readiness
- [ ] Set up CI pipeline stages:
  - format + clippy
  - unit + integration + nextest
  - benchmark smoke
  - security checks (`audit`, `deny`)
- [ ] Add release profile tuning (LTO, codegen units, strip symbols as needed).
- [ ] Create versioned release checklist and changelog template.
- [ ] Tag release candidate and run final validation on macOS + Linux.

## Phase 12: Definition of Done
- [ ] All critical checklist items moved to `[x]`.
- [ ] Performance targets achieved on at least one medium dataset profile.
- [ ] Manual test checklist passed through Dioxus UI end-to-end.
- [ ] MCP API contract documented and validated with at least one external MCP client.
- [ ] Deployment docs verified by a clean-machine install test.

## Execution Workflow Rule
- [ ] During implementation, update checklist status in this file continuously:
  - Current active step must be marked `[-]`
  - Completed steps must be marked `[x]`
  - Not-yet-started steps remain `[ ]`
