#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_BINARY_PATH="$ROOT_DIR/target/release/cid"
PACKAGED_BINARY_PATH="$DIST_DIR/cid"
SQUASHFS_IMAGE_PATH="$DIST_DIR/ui.squashfs"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/build-release.sh

Builds a self-contained release binary with the web UI appended as SquashFS.
EOF
}

log() {
  printf '==> %s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || fail "required tool not found: $tool"
}

append_u32_le() {
  local output_path="$1"
  local value="$2"
  local escaped

  escaped="$(printf '\\%03o\\%03o\\%03o\\%03o' \
    $((value & 255)) \
    $(((value >> 8) & 255)) \
    $(((value >> 16) & 255)) \
    $(((value >> 24) & 255)))"
  printf '%b' "$escaped" >> "$output_path"
}

assert_packaged_binary_magic() {
  local binary_path="$1"
  local actual_magic

  actual_magic="$(tail -c 8 "$binary_path")"
  [[ "$actual_magic" == "SQUASHFS" ]] || fail "packaged binary is missing the SQUASHFS trailer magic"
}

build_release_binary() {
  require_tool cargo
  require_tool pnpm
  require_tool mksquashfs
  require_tool cp
  require_tool rm
  require_tool mkdir
  require_tool tail
  require_tool wc
  require_tool chmod

  mkdir -p "$DIST_DIR"
  rm -f "$PACKAGED_BINARY_PATH" "$SQUASHFS_IMAGE_PATH"

  log "building production web ui"
  pnpm --dir "$ROOT_DIR/ui" build

  log "packing ui/dist into zstd-compressed squashfs"
  mksquashfs "$ROOT_DIR/ui/dist" "$SQUASHFS_IMAGE_PATH" -noappend -quiet -all-root -comp zstd

  local squashfs_size
  squashfs_size="$(wc -c < "$SQUASHFS_IMAGE_PATH")"
  [[ "$squashfs_size" -le 4294967295 ]] || fail "squashfs image is too large for 32-bit trailer size"

  log "building release cid binary"
  cargo build --release -p cid-server --locked

  log "appending embedded web ui to release binary"
  cp "$RELEASE_BINARY_PATH" "$PACKAGED_BINARY_PATH"
  cat "$SQUASHFS_IMAGE_PATH" >> "$PACKAGED_BINARY_PATH"
  append_u32_le "$PACKAGED_BINARY_PATH" "$squashfs_size"
  printf 'SQUASHFS' >> "$PACKAGED_BINARY_PATH"
  chmod +x "$PACKAGED_BINARY_PATH"

  assert_packaged_binary_magic "$PACKAGED_BINARY_PATH"
  log "packaged binary written to $PACKAGED_BINARY_PATH"
}

main() {
  if [[ "${1:-}" == "--help" ]]; then
    usage
    return
  fi

  [[ $# -eq 0 ]] || fail "build-release.sh does not accept additional arguments"
  build_release_binary
}

main "$@"
