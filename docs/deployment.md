# Deployment

## Docker
- Build: `docker compose build`
- Run: `REPO_PATH=/absolute/path/to/repo docker compose up -d`
- Optional model mount/env:
  - `MODEL_DIR=/absolute/path/to/models`
  - `MODEL_PATH=/models/embedding.onnx`
- MCP endpoint: `http://127.0.0.1:38080/mcp`
- Repo mount is read-only (`/repo:ro`).
- End-to-end wiring verification:
  - `make verify-docker`
  - Checks: read-only repo mount, MCP health, Qdrant connectivity, model path presence.
  - Default `REQUIRE_MODEL=0` (non-blocking if model file is missing).
  - Strict model check: `REQUIRE_MODEL=1 make verify-docker`

## Qdrant Modes
- Sidecar mode: provided by `docker-compose.yml`.
- Embedded mode: supported by app config (to be wired in runtime config phase).

## macOS Native
- Install and load launch agent:
  - `./scripts/install-macos.sh`
- Launch agent plist:
  - `deploy/macos/com.codivex.mcp-server.plist`
- Background tuning (included in plist):
  - `ProcessType=Background`
  - `Nice=10` (lower CPU priority for desktop responsiveness)

## Shutdown Behavior
- Graceful shutdown on `SIGINT`/`SIGTERM`:
  - stop background indexing loops,
  - persist `.codivex/runtime-state.json`,
  - finish HTTP shutdown cleanly.
