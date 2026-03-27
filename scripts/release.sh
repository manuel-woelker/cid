#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-}"
VERSION="${2:-}"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/release.sh prepare <version>
  ./scripts/release.sh tag <version>

This script is intentionally minimal for now.
`prepare` validates the workspace and version format.
`tag` also creates an annotated git tag after validation.
EOF
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_clean_worktree() {
  if [[ -n "$(git -C "$ROOT_DIR" status --short)" ]]; then
    fail "git worktree is not clean"
  fi
}

validate_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || fail "invalid version: $1"
}

run_checks() {
  "$ROOT_DIR/scripts/check-code.sh"
}

create_tag() {
  local tag_name="v$1"
  git -C "$ROOT_DIR" rev-parse -q --verify "refs/tags/$tag_name" >/dev/null 2>&1 \
    && fail "tag already exists: $tag_name"
  git -C "$ROOT_DIR" tag -a "$tag_name" -m "Release $tag_name"
  printf 'created tag %s\n' "$tag_name"
}

if [[ "$MODE" == "--help" || "$MODE" == "-h" || -z "$MODE" ]]; then
  usage
  exit 0
fi

validate_version "$VERSION"
require_clean_worktree
run_checks

case "$MODE" in
  prepare)
    printf 'release checks passed for version %s\n' "$VERSION"
    ;;
  tag)
    create_tag "$VERSION"
    ;;
  *)
    fail "unknown mode: $MODE"
    ;;
esac
