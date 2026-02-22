#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_ROOT="$(mktemp -d /tmp/codivex-cli-install.XXXXXX)"
trap 'rm -rf "${INSTALL_ROOT}"' EXIT

cd "${ROOT_DIR}"
cargo install --path crates/mcp-code-indexer --locked --force --root "${INSTALL_ROOT}"
"${INSTALL_ROOT}/bin/codivex-mcp" --help >/dev/null
echo "codivex-mcp install smoke test passed"
