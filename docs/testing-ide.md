# IDE and MCP Client Testing Guide

This guide covers end-to-end validation from real MCP clients.

For client configuration snippets (Cursor/Claude/Zed), see `docs/mcp-local-client-setup.md`.

## 1) Start Server
- HTTP/SSE mode:
  - `make run-mcp`
- stdio mode:
  - `make run-stdio`
- rmcp stdio adapter (feature-gated):
  - `make run-rmcp-stdio`
- Docker mode:
  - `REPO_PATH=/absolute/path/to/project docker compose up -d --build`
  - verify runtime wiring: `make verify-docker`

## 2) Index a Project
- Add/select project:
  - `cargo run -p codivex-mcp -- add-repo /absolute/path/to/project`
- Build indexes:
  - `cargo run -p codivex-mcp -- index-now`
- Verify:
  - `cargo run -p codivex-mcp -- status`

## 3) Raw MCP Validation
- `initialize`:
  - `POST /mcp` with method `initialize`
- `tools/list`:
  - confirm `searchCode` and `openLocation`
- `tools/call`:
  - call `searchCode`, then `openLocation`
- SSE:
  - `GET /mcp/sse?query=...&top_k=...`

Example raw call sequence:
```bash
curl -sS http://127.0.0.1:38080/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

curl -sS http://127.0.0.1:38080/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"searchCode","arguments":{"query":"PramukhIME","top_k":1}}}'
```

## 4) Client-Specific Manual Checks
- Cursor:
  - connect to HTTP endpoint or stdio command
  - run exact symbol + semantic query
- Claude Code/Desktop:
  - register MCP server
  - verify tool discovery and result quality
- Zed:
  - stdio command mode
  - verify initialize/tools/list/tools/call flow
- Replit:
  - HTTP endpoint mode
  - verify auth + search/open flow

Use `docs/mcp-client-matrix.md` to track pass/fail by client/version.

## 5) Multi-Project Scope
- Use `repoFilter` in `searchCode` params, or header `x-codivex-project`.
- For relative repo names, configure optional roots:
  - `CODIVEX_PROJECT_ROOTS=/path/root1:/path/root2`

## 6) Privacy-Safe Local Testing
- Keep local/private paths out of tracked files.
- Prefer env vars for dataset paths and keep report redaction enabled:
  - `BENCHMARK_REDACT_PATH=true`
  - `QUALITY_REDACT_PATH=true`
