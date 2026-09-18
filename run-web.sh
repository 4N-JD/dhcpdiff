#!/usr/bin/env bash
# Launch the dhcpdiff web UI from the repository root.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

HOST="${DHCPDIFF_WEB_HOST:-0.0.0.0}"
PORT="${DHCPDIFF_WEB_PORT:-8080}"
BIN="${DHCPDIFF_BIN:-$ROOT/target/release/dhcpdiff}"

if [[ ! -x "$BIN" ]]; then
  echo "Building dhcpdiff (release)…"
  cargo build --release
  BIN="$ROOT/target/release/dhcpdiff"
fi

if [[ ! -x "$ROOT/web/.venv/bin/uvicorn" ]]; then
  echo "Creating web/.venv and installing deps…"
  python3 -m venv "$ROOT/web/.venv"
  "$ROOT/web/.venv/bin/pip" install -r "$ROOT/web/requirements.txt"
fi

export DHCPDIFF_BIN="$BIN"
export PYTHONPATH="$ROOT/web"

echo "dhcpdiff: $DHCPDIFF_BIN"
echo "Open http://${HOST}:${PORT}"
exec "$ROOT/web/.venv/bin/uvicorn" app.main:app --host "$HOST" --port "$PORT"
