#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required"
  exit 1
fi

COMPOSE_BIN="docker compose"
if ! ${COMPOSE_BIN} version >/dev/null 2>&1; then
  if command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_BIN="docker-compose"
  else
    echo "docker compose (plugin) or docker-compose is required"
    exit 1
  fi
fi

REQUIRE_MODEL="${REQUIRE_MODEL:-0}"
MCP_HEALTH_URL="${MCP_HEALTH_URL:-http://127.0.0.1:38080/health}"
QDRANT_COLLECTIONS_URL="${QDRANT_COLLECTIONS_URL:-http://127.0.0.1:6333/collections}"

echo "[verify] starting docker services"
${COMPOSE_BIN} up -d --build

cleanup() {
  if [[ "${KEEP_CONTAINERS:-0}" != "1" ]]; then
    ${COMPOSE_BIN} down >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "[verify] waiting for MCP health endpoint: ${MCP_HEALTH_URL}"
for _ in $(seq 1 60); do
  if curl -fsS "${MCP_HEALTH_URL}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -fsS "${MCP_HEALTH_URL}" >/dev/null

echo "[verify] checking Qdrant connectivity: ${QDRANT_COLLECTIONS_URL}"
curl -fsS "${QDRANT_COLLECTIONS_URL}" >/dev/null

echo "[verify] validating read-only /repo mount"
server_container_id="$(${COMPOSE_BIN} ps -q mcp-server | head -n 1 | tr -d '\r')"
if [[ -z "${server_container_id}" ]]; then
  echo "unable to locate mcp-server container id from compose"
  ${COMPOSE_BIN} ps || true
  exit 1
fi

mount_mode="$(docker inspect "${server_container_id}" --format '{{range .Mounts}}{{if eq .Destination "/repo"}}{{.RW}}{{end}}{{end}}' | tr -d '\r')"
if [[ "${mount_mode}" != "false" ]]; then
  echo "expected /repo mount to be read-only, got RW=${mount_mode}"
  exit 1
fi

model_path="$(${COMPOSE_BIN} exec -T mcp-server /bin/sh -lc 'printf "%s" "${MODEL_PATH}"')"
echo "[verify] model path in container: ${model_path}"
if [[ "${REQUIRE_MODEL}" == "1" ]]; then
  ${COMPOSE_BIN} exec -T mcp-server /bin/sh -lc 'test -f "${MODEL_PATH}"'
else
  if ! ${COMPOSE_BIN} exec -T mcp-server /bin/sh -lc 'test -f "${MODEL_PATH}"'; then
    echo "[verify] model file missing (allowed because REQUIRE_MODEL=${REQUIRE_MODEL})"
  fi
fi

echo "[verify] docker runtime wiring checks passed"
