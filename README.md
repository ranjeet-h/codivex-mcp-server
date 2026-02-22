# codivex-mcp-server

Rust workspace bootstrap for the local MCP code search engine project.

## Documentation Index
- Deployment: [`docs/deployment.md`](docs/deployment.md)
- Security: [`docs/security.md`](docs/security.md)
- CLI Guide (install/run/update): [`docs/cli.md`](docs/cli.md)
- IDE/MCP Testing: [`docs/testing-ide.md`](docs/testing-ide.md)
- Local Client Setup (Cursor/Claude/Zed): [`docs/mcp-local-client-setup.md`](docs/mcp-local-client-setup.md)
- Performance/Quality Testing: [`docs/testing-performance.md`](docs/testing-performance.md)
- MCP Client Matrix: [`docs/mcp-client-matrix.md`](docs/mcp-client-matrix.md)
- Tuning Matrix: [`benchmarks/production-tuning-matrix.md`](benchmarks/production-tuning-matrix.md)

## Toolchain
- `rustc 1.93.1`
- `cargo 1.93.1`

## Workspace Crates
- `crates/mcp-server` (binary)
- `crates/ui-dioxus` (binary)
- `crates/indexer` (library)
- `crates/mcp-code-indexer` (CLI package `codivex-mcp`)
- `crates/search-core` (library)
- `crates/embeddings` (library)
- `crates/common` (library)

## Quick Start
```bash
cargo check --workspace
```

## CLI
```bash
cargo run -p codivex-mcp -- add-repo /absolute/path/to/repo
cargo run -p codivex-mcp -- index-now
cargo run -p codivex-mcp -- status
```

## Transports
```bash
# HTTP JSON-RPC + SSE server
make run-mcp

# stdio MCP transport
make run-stdio

# rmcp-based stdio adapter (feature-gated)
make run-rmcp-stdio
```

## Docker Runtime Check
```bash
# verifies MCP health, Qdrant connectivity, and read-only repo mount
make verify-docker
```

## IDE Test Flow
Use [`docs/testing-ide.md`](docs/testing-ide.md) for client-by-client setup and
[`docs/mcp-client-matrix.md`](docs/mcp-client-matrix.md) to track pass/fail.

## Optional Project Roots
For relative project names in UI/MCP requests, set:
```bash
export CODIVEX_PROJECT_ROOTS=/path/projects-a:/path/projects-b
```
