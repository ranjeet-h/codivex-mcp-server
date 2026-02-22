# MCP Client Compatibility Matrix

## Status Legend
- `PASS`: validated in this repository (automated or manual evidence)
- `PENDING`: requires local client app/manual validation
- `BLOCKED`: cannot validate due missing environment dependency

## Core Protocol Validation (Automated)
| Capability | Transport | Evidence | Status |
|---|---|---|---|
| `initialize` | HTTP JSON-RPC | `crates/mcp-server/tests/rpc_sse_integration.rs` | PASS |
| `tools/list` | HTTP JSON-RPC | `crates/mcp-server/tests/rpc_sse_integration.rs` | PASS |
| `tools/call` | HTTP JSON-RPC | `crates/mcp-server/tests/rpc_sse_integration.rs` | PASS |
| `searchCode` stream | SSE | `crates/mcp-server/tests/rpc_sse_integration.rs` | PASS |
| JSON-RPC over ws | WebSocket | `crates/mcp-server/tests/rpc_sse_integration.rs` (`/mcp/ws`) | PASS |
| JSON-RPC over stdio | stdio subprocess | `crates/mcp-server/tests/stdio_integration.rs` | PASS |

## External Client Matrix (Manual)
| Client | Transport | Setup | Status | Notes |
|---|---|---|---|---|
| Cursor | HTTP or stdio | Point to `/mcp` or `mcp_stdio` | PENDING | Validate tool discovery + project scoping header behavior |
| Claude Code/Desktop | HTTP or stdio | Register MCP endpoint | PENDING | Validate streamed result consumption |
| Zed | stdio | Configure local command `mcp_stdio` | PENDING | Validate initialize/tools/list roundtrip |
| Replit | HTTP | Expose local endpoint via tunnel if needed | PENDING | Validate auth token mode and latency |

## Validation Steps Per Client
1. Connect client to MCP server (`/mcp`, `/mcp/ws`, or stdio).
2. Confirm `initialize` success and tool discovery (`tools/list` includes `searchCode`, `openLocation`).
3. Run query `iso to date` and verify at least one relevant chunk.
4. Run exact symbol query and verify line range accuracy.
5. Validate project scoping (`repoFilter` or `x-codivex-project`) across at least two indexed repos.
6. Capture output screenshot/log and update this matrix.

## Known Caveats
- Docker runtime wiring now validated via `make verify-docker`.
- Strict model-file validation still depends on local model mount (`REQUIRE_MODEL=1 make verify-docker`).
- External app-specific integration can differ by app version; record app version/date with each validation result.
