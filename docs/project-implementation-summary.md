# Codivex MCP: Implementation Summary

## 1. What We Built

Codivex MCP is a local-first, Rust-based code indexing and retrieval system designed for agentic IDE workflows.  
It provides:

- A local MCP server for tool-based code retrieval.
- Hybrid search across indexed repositories.
- Project-scoped retrieval so agents only search the intended repo.
- A Rust admin UI (Dioxus) for indexing, diagnostics, and manual testing.
- CLI-based repo/index management.
- Docker and native runtime options.

Primary goal: provide fast, precise code context to IDE agents without sending source code to external services.

## 2. Why This Is Better

- Better relevance than single-method search by combining exact symbol, lexical BM25, and semantic retrieval.
- Better speed for large repos via incremental indexing and per-project index isolation.
- Better privacy by default with local-only execution and read-only source access patterns.
- Better agent interoperability through MCP `initialize`, `tools/list`, `tools/call`, HTTP/SSE, stdio, and WebSocket fallback.
- Better operational clarity via admin UI, metrics, telemetry, benchmark tooling, and SLO validation commands.

## 3. System Architecture

### 3.1 Ingestion and Indexing

- File discovery and ignore-aware scanning (`ignore` crate).
- File watching for background incremental updates (`notify` crate).
- AST-based chunk extraction with Tree-sitter and language parsers.
- Chunk fingerprinting and deduplication for efficient re-indexing.
- Per-project index persistence:
  - Tantivy lexical index.
  - Qdrant vector collection.
  - Local project catalog/state in `.codivex/`.

### 3.2 Retrieval Pipeline

- Exact symbol lookup (fast path).
- Lexical retrieval (Tantivy BM25).
- Semantic retrieval (embeddings + Qdrant ANN or local fallback path).
- Reciprocal Rank Fusion (RRF) to merge lexical and semantic rankings.
- Optional rerank tier (`MCP_RETRIEVAL_TIER=hybrid_rerank`).

### 3.3 MCP Layer

- JSON-RPC endpoint: `/mcp`
- SSE endpoint: `/mcp/sse`
- WebSocket endpoint: `/mcp/ws`
- Stdio transport binaries:
  - `mcp_stdio`
  - `rmcp_stdio` (feature-gated `rmcp-integration`)
- Tooling exposed via `tools/list`:
  - `searchCode`
  - `openLocation`
- Tool metadata includes input schema, output schema, and read-only/idempotent hints.

### 3.4 Admin and Ops

- Dioxus admin UI at `/admin`.
- Port conflict handling with deterministic local fallback allocation.
- Runtime diagnostics and health endpoints:
  - `/health`
  - `/metrics`
  - `/port-diagnostics`
  - telemetry SSE endpoints.
- Docker runtime verification script for health, Qdrant connectivity, and read-only mount checks.

## 4. Implemented Components

- `crates/mcp-server`: MCP server, transports, handlers, telemetry, benchmark/load binaries.
- `crates/indexer`: scanner, parser registry, chunking, incremental/sync support.
- `crates/search-core`: lexical/vector/fusion primitives and retrieval harnesses.
- `crates/embeddings`: ONNX runtime integration, batching, tokenizer support.
- `crates/ui-dioxus`: admin UI and UI-side backend routes.
- `crates/common`: schemas, project/state helpers, shared models.
- `crates/mcp-code-indexer` (package name `codivex-mcp`): CLI for repo/index lifecycle.

## 5. Libraries and Technologies Used

### Core Runtime and API

- Rust 1.93.1
- Tokio
- Axum
- Tower / Tower HTTP
- Serde / Serde JSON
- Schemars
- Jsonschema
- Tracing / Tracing Subscriber (JSON logs)

### Search and Indexing

- Tree-sitter + language grammars:
  - Rust, C, C++, C#, Go, Haskell, Java, JavaScript, Kotlin, PHP, Python, Ruby, Swift, TypeScript
- Tantivy
- Qdrant client
- DashMap
- LRU
- AHash
- Parking Lot

### Embeddings

- `ort` (ONNX Runtime Rust bindings)
- `tokenizers`
- Rayon

### UI and DX

- Dioxus
- Dioxus Router
- Dioxus SSR
- Dioxus Logger
- Clap (CLI)

### Observability, Testing, and Quality

- Metrics + Prometheus exporter
- Criterion benchmarks
- Proptest
- Insta snapshots
- Reqwest
- Tok io Tungstenite (tests for WS path)
- Cargo Audit
- Cargo Deny

## 6. Key Functional Capabilities Delivered

- Multi-project index catalog with selected active project.
- Project-scoped `searchCode` retrieval through `repoFilter` or header scoping.
- `openLocation` with real file read and line-range validation.
- Background watcher startup and incremental indexing hooks.
- RRF-based hybrid search with configurable retrieval tier.
- Stdio + HTTP/SSE + WS transport coverage.
- Docker verification workflow and runtime hardening fixes.
- Local client setup docs for Cursor, Claude Code, and Zed.
- Privacy-safe report defaults (path redaction support for benchmark/quality outputs).

## 7. CLI (Project-Named)

CLI package/binary name:

- `codivex-mcp`

Supported commands:

- `add-repo`
- `remove-repo`
- `list-repos`
- `index-now`
- `status`

CLI docs:

- `docs/cli.md`

## 8. Testing and Validation Performed

- Workspace formatting, check, and tests pass for current codebase.
- MCP protocol integration tests include:
  - `initialize`
  - `tools/list`
  - `tools/call`
  - SSE result/done ordering
  - WebSocket JSON-RPC fallback
  - stdio initialize flow
- CLI integration tests pass.
- Docker verification (`make verify-docker`) passes with default non-blocking model check.
- Benchmark matrix, load test, and quality harness commands are available and executed.

## 9. Documents and Operational Guides Added

- `README.md` documentation index and run flows.
- `docs/mcp-local-client-setup.md` for local keyless MCP setup in IDE clients.
- `docs/testing-ide.md` for manual MCP/IDE validation.
- `docs/testing-performance.md` for benchmark/quality/SLO workflows.
- `docs/deployment.md` for runtime and Docker behavior.
- `docs/mcp-client-matrix.md` for client compatibility tracking.
- `docs/cli.md` for CLI install/run/update usage.

## 10. Current Status and Remaining Gaps

Completed major tracks:

- Transport matrix (HTTP/SSE, stdio, WS, feature-gated RMCP stdio).
- Hybrid retrieval pipeline + optional rerank mode.
- Per-project indexing and scoping controls.
- Dioxus admin UI alignment for operational flows.
- CLI packaging and install smoke tests.

Still pending or partially validated:

- Full external client matrix completion with real app-by-app pass evidence.
- Strict SLO pass across all benchmark profiles/hardware classes.
- Large-scale envelope validation target (1M vectors @ 1536 dims, 10M+ LOC profiling).
- macOS tray app with full launchd control integration.
- Strict Docker model-file validation in all environments (`REQUIRE_MODEL=1`) where model mount exists.

## 11. Practical Summary

This project now provides a real, local MCP code retrieval system with production-oriented fundamentals:

- modular Rust workspace,
- hybrid retrieval,
- project-scoped search for agent IDEs,
- usable admin/testing surfaces,
- and clear operational tooling.

It is suitable as an open-source base for continued hardening toward stricter scale/SLO and full client certification.
