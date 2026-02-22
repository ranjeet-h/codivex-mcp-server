#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLIST_SRC="$ROOT_DIR/deploy/macos/com.codivex.mcp-server.plist"
PLIST_DST="$HOME/Library/LaunchAgents/com.codivex.mcp-server.plist"

cargo build --release -p mcp-server
install -m 0755 "$ROOT_DIR/target/release/mcp-server" /usr/local/bin/mcp-server
cp "$PLIST_SRC" "$PLIST_DST"

launchctl unload "$PLIST_DST" >/dev/null 2>&1 || true
launchctl load "$PLIST_DST"

echo "Installed and loaded com.codivex.mcp-server"

