#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export BIND_ADDR="${BIND_ADDR:-0.0.0.0:8090}"
export RUST_PIC_BIN="${RUST_PIC_BIN:-$ROOT_DIR/target/debug/rust-pic}"
export WORKSPACES_DIR="${WORKSPACES_DIR:-$ROOT_DIR/workspaces}"
export LXCAT_DIR="${LXCAT_DIR:-$ROOT_DIR/xsec}"

cd "$ROOT_DIR"
cargo build -p rust-pic

pids=()

cleanup() {
  trap - INT TERM EXIT
  if ((${#pids[@]} > 0)); then
    kill "${pids[@]}" 2>/dev/null || true
    wait "${pids[@]}" 2>/dev/null || true
  fi
}

trap cleanup INT TERM EXIT

(
  cd "$ROOT_DIR"
  cargo run -p server
) &
pids+=("$!")

(
  cd "$ROOT_DIR/web"
  npm run dev
) &
pids+=("$!")

wait -n "${pids[@]}"
status=$?
cleanup
exit "$status"
