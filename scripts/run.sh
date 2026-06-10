#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_BIN="$ROOT_DIR/target/release/server"

if [[ ! -x "$SERVER_BIN" ]]; then
  echo "Missing server binary: $SERVER_BIN" >&2
  echo "Run scripts/build.sh first." >&2
  exit 1
fi

export BIND_ADDR="${BIND_ADDR:-0.0.0.0:8090}"
export RUST_PIC_BIN="${RUST_PIC_BIN:-$ROOT_DIR/target/release/rust-pic}"
export WORKSPACES_DIR="${WORKSPACES_DIR:-$ROOT_DIR/workspaces}"
export LXCAT_DIR="${LXCAT_DIR:-$ROOT_DIR/xsec}"

if [[ -d "$ROOT_DIR/web/dist" ]]; then
  export WEB_DIST="${WEB_DIST:-$ROOT_DIR/web/dist}"
fi

cd "$ROOT_DIR"
exec "$SERVER_BIN"
