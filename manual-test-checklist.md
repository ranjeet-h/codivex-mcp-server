# Manual QA Checklist

Use this checklist for end-to-end validation through the Dioxus admin UI (`/admin`).
Status legend:
- `[ ]` Not started
- `[-]` In progress
- `[x]` Completed

## Environment Setup
- [ ] Verify Rust binaries run on local host:
  - `make run` for MCP server
  - `make run-ui` for Dioxus UI
- [ ] Confirm unique runtime ports in `/port-diagnostics` for both services.
- [ ] Confirm benchmark artifacts exist:
  - `benchmarks/latest-report.json`
  - `benchmarks/load-test-report.json`
- [ ] Open UI endpoint and confirm `Benchmark Summary` panel is visible.

## Core Manual Flow
- [ ] Add repository path and trigger initial index from UI controls.
- [ ] Execute exact symbol query from Search Playground.
- [ ] Execute semantic query from Search Playground.
- [ ] Validate SSE result events are shown before completion.
- [ ] Validate SSE `done` completion event.
- [ ] Validate `openLocation` response path and line range consistency.
- [ ] Modify a file in indexed repo and validate incremental re-index behavior.

## Edge Cases
- [ ] Empty query returns validation error.
- [ ] Oversized query returns bounded/handled response.
- [ ] Unsupported file type does not break indexing.
- [ ] Qdrant unavailable state is surfaced correctly.
- [ ] Startup port conflict is auto-resolved and reflected in diagnostics.

## Performance Validation
- [ ] Run `make bench` and inspect suite values in UI.
- [ ] Run `make load-test` and inspect API/SSE success and p50/p95/p99 in UI.
- [ ] Record observed latencies against targets in `benchmarks/targets.md`.

## Sign-off
- [ ] All core flow checks passed.
- [ ] All edge-case checks passed.
- [ ] Manual QA sign-off recorded with date and tester name.
