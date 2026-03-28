#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/Cargo.toml"

if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "error: workspace manifest not found at $MANIFEST_PATH" >&2
  exit 1
fi

run_step() {
  local label="$1"
  shift

  printf '==> %s\n' "$label"
  "$@"
}

run_parallel_step() {
  local label="$1"
  shift

  (
    printf '==> %s\n' "$label"
    "$@"
  ) &
  PARALLEL_PIDS+=("$!")
}

wait_for_parallel_steps() {
  local status=0

  for pid in "${PARALLEL_PIDS[@]}"; do
    if ! wait "$pid"; then
      status=1
    fi
  done

  return "$status"
}

run_step "cargo fmt" cargo fmt --manifest-path "$MANIFEST_PATH" --all --check
run_step "cargo build" cargo build --manifest-path "$MANIFEST_PATH" --workspace --all-targets --all-features
run_step "cargo clippy" cargo clippy --manifest-path "$MANIFEST_PATH" --workspace --all-targets --all-features -- -D warnings
run_step "cargo test" cargo test --manifest-path "$MANIFEST_PATH" --workspace --all-targets --all-features

if [[ -f "$ROOT_DIR/ui/package.json" ]]; then
  run_step "pnpm format" pnpm --dir "$ROOT_DIR/ui" format
  run_step "pnpm lint" pnpm --dir "$ROOT_DIR/ui" lint
  PARALLEL_PIDS=()
  run_parallel_step "pnpm typecheck" pnpm --dir "$ROOT_DIR/ui" typecheck
  run_parallel_step "pnpm build" pnpm --dir "$ROOT_DIR/ui" build
  run_parallel_step "pnpm test" pnpm --dir "$ROOT_DIR/ui" test:run
  wait_for_parallel_steps
fi
