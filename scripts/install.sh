#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage: scripts/install.sh [cargo-install-args...]

Installs the `cid` binary from `crates/server` using `cargo install --path`.
Any extra arguments are forwarded to `cargo install`.
EOF
  exit 0
fi

exec cargo install --path "$ROOT_DIR/crates/server" "$@"
