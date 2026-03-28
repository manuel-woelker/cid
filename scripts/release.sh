#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_BRANCH="${CID_RELEASE_BRANCH:-main}"
WAIT_ATTEMPTS="${CID_RELEASE_WAIT_ATTEMPTS:-30}"
WAIT_SECONDS="${CID_RELEASE_WAIT_SECONDS:-10}"
DRY_RUN=0
MODE="publish"
TARGET_VERSION=""

CRATE_DIRS=(
  "crates/base"
  "crates/pal"
  "crates/daemon"
  "crates/web"
  "crates/server"
)

CRATE_PACKAGES=(
  "cid-base"
  "cid-pal"
  "cid-daemon"
  "cid-web"
  "cid-server"
)

usage() {
  cat <<'EOF'
Usage:
  ./scripts/release.sh release <version>
  ./scripts/release.sh prepare <version>
  ./scripts/release.sh publish [--dry-run]
  ./scripts/release.sh [--dry-run]

Options:
  --dry-run    Run release checks without publishing crates or creating tags.
  --help       Show this help.
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

workspace_version() {
  sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)"$/\1/p' "$ROOT_DIR/Cargo.toml" | head -n1
}

validate_version() {
  local version="$1"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || fail "invalid version: $version"
}

current_branch() {
  git -C "$ROOT_DIR" symbolic-ref --quiet --short HEAD || true
}

assert_clean_worktree() {
  if [[ -n "$(git -C "$ROOT_DIR" status --short)" ]]; then
    fail "git worktree is not clean"
  fi
}

assert_release_branch() {
  local branch
  branch="$(current_branch)"
  [[ -n "$branch" ]] || fail "release requires a checked out branch, not a detached HEAD"
  [[ "$branch" == "$RELEASE_BRANCH" ]] || fail "release must run from branch '$RELEASE_BRANCH' (current: '$branch')"
}

assert_publish_auth() {
  if [[ -n "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    return
  fi

  if [[ -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" ]] || [[ -f "${CARGO_HOME:-$HOME/.cargo}/credentials" ]]; then
    return
  fi

  fail "crates.io credentials not found; set CARGO_REGISTRY_TOKEN or run cargo login"
}

assert_tag_absent() {
  local tag_name="$1"

  if git -C "$ROOT_DIR" rev-parse -q --verify "refs/tags/$tag_name" >/dev/null 2>&1; then
    fail "tag already exists locally: $tag_name"
  fi
}

crate_version_exists_on_crates_io() {
  local package_name="$1"
  local version="$2"
  local url="https://crates.io/api/v1/crates/$package_name/$version"

  curl --fail --silent --show-error "$url" >/dev/null 2>&1
}

assert_release_version_available() {
  local version="$1"
  local package_name

  for package_name in "${CRATE_PACKAGES[@]}"; do
    if crate_version_exists_on_crates_io "$package_name" "$version"; then
      fail "$package_name $version already exists on crates.io; bump the shared version before publishing"
    fi
  done
}

assert_shared_version() {
  local expected_version="$1"
  local actual_version

  actual_version="$(workspace_version)"
  [[ -n "$actual_version" ]] || fail "missing workspace version in Cargo.toml"
  [[ "$actual_version" == "$expected_version" ]] || fail "Cargo.toml uses version $actual_version, expected $expected_version"
}

assert_internal_dependency_versions() {
  local expected_version="$1"
  local crate_dir
  local manifest_path

  for crate_dir in "${CRATE_DIRS[@]}"; do
    manifest_path="$ROOT_DIR/$crate_dir/Cargo.toml"

    while IFS= read -r dependency_line; do
      local dependency_name
      dependency_name="$(printf '%s\n' "$dependency_line" | sed -nE 's/^([a-z0-9-]+)[[:space:]]*=.*/\1/p')"
      local dependency_version
      dependency_version="$(printf '%s\n' "$dependency_line" | sed -nE 's/.*version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p')"

      [[ -n "$dependency_version" ]] || fail "$manifest_path is missing a version for internal dependency $dependency_name"
      [[ "$dependency_version" == "$expected_version" ]] || fail "$manifest_path pins $dependency_name to $dependency_version, expected $expected_version"
    done < <(grep -E '^[a-z0-9-]+[[:space:]]*=.*path[[:space:]]*=' "$manifest_path" || true)
  done
}

update_workspace_version() {
  local version="$1"

  sed -E -i \
    -e "/^\[workspace\.package\]$/, /^\[/ s/^version[[:space:]]*=[[:space:]]*\"[^\"]+\"$/version = \"$version\"/" \
    "$ROOT_DIR/Cargo.toml"
}

update_internal_dependency_versions() {
  local version="$1"
  local crate_dir
  local manifest_path

  for crate_dir in "${CRATE_DIRS[@]}"; do
    manifest_path="$ROOT_DIR/$crate_dir/Cargo.toml"
    sed -E -i \
      -e "/path[[:space:]]*=/ s/version[[:space:]]*=[[:space:]]*\"[^\"]+\"/version = \"$version\"/g" \
      "$manifest_path"
  done
}

prepare_release_version() {
  local version="$1"
  local print_manual_next_step="${2:-1}"
  local current_version

  validate_version "$version"
  current_version="$(workspace_version)"
  [[ "$current_version" != "$version" ]] || fail "workspace already uses version $version"

  require_tool git
  require_tool sed
  assert_clean_worktree

  log "updating shared workspace version to $version"
  update_workspace_version "$version"
  update_internal_dependency_versions "$version"

  assert_shared_version "$version"
  assert_internal_dependency_versions "$version"

  log "prepared shared release version $version"
  if [[ "$print_manual_next_step" -eq 1 ]]; then
    log "review the manifest changes, commit them, then run ./scripts/release.sh publish"
  fi
}

commit_release_version_bump() {
  local version="$1"

  cargo update --workspace --offline
  git -C "$ROOT_DIR" add "$ROOT_DIR/Cargo.toml" "$ROOT_DIR/Cargo.lock"
  git -C "$ROOT_DIR" add "$ROOT_DIR"/crates/*/Cargo.toml
  git -C "$ROOT_DIR" commit -m "chore(release): bump version to $version"
}

run_repo_checks() {
  log "running repository checks"
  "$ROOT_DIR/scripts/check-code.sh"
}

wait_for_crate_version() {
  local package_name="$1"
  local version="$2"
  local attempt
  local url="https://crates.io/api/v1/crates/$package_name/$version"

  for ((attempt = 1; attempt <= WAIT_ATTEMPTS; attempt += 1)); do
    if curl --fail --silent --show-error "$url" >/dev/null; then
      return
    fi

    sleep "$WAIT_SECONDS"
  done

  fail "timed out waiting for $package_name $version to become available on crates.io"
}

publish_crates() {
  local version="$1"
  local crate_dir
  local package_name
  local index

  for index in "${!CRATE_DIRS[@]}"; do
    crate_dir="${CRATE_DIRS[$index]}"
    package_name="${CRATE_PACKAGES[$index]}"

    log "publishing $package_name $version"
    cargo publish \
      --manifest-path "$ROOT_DIR/$crate_dir/Cargo.toml" \
      --locked

    log "waiting for $package_name $version to appear on crates.io"
    wait_for_crate_version "$package_name" "$version"
  done
}

create_and_push_tag() {
  local tag_name="$1"
  log "creating annotated tag $tag_name"
  git -C "$ROOT_DIR" tag -a "$tag_name" -m "Release $tag_name"

  log "pushing tag $tag_name"
  git -C "$ROOT_DIR" push origin "$tag_name"
}

run_pre_release_checks() {
  local version="$1"
  local tag_name="$2"

  log "running pre-release checks for version $version"
  require_tool git
  require_tool cargo
  require_tool curl
  require_tool sed
  require_tool grep

  assert_release_branch
  assert_clean_worktree
  assert_shared_version "$version"
  assert_internal_dependency_versions "$version"
  assert_release_version_available "$version"
  run_repo_checks
  assert_clean_worktree

  if [[ "$DRY_RUN" -eq 0 ]]; then
    assert_publish_auth
    assert_tag_absent "$tag_name"
  fi
}

parse_args() {
  if [[ $# -gt 0 ]]; then
    case "$1" in
      release|prepare|publish)
        MODE="$1"
        shift
        ;;
      --help)
        usage
        exit 0
        ;;
    esac
  fi

  case "$MODE" in
    release)
      [[ $# -eq 1 ]] || fail "release requires exactly one version argument"
      TARGET_VERSION="$1"
      ;;
    prepare)
      [[ $# -eq 1 ]] || fail "prepare requires exactly one version argument"
      TARGET_VERSION="$1"
      ;;
    publish)
      while [[ $# -gt 0 ]]; do
        case "$1" in
          --dry-run)
            DRY_RUN=1
            ;;
          --help)
            usage
            exit 0
            ;;
          *)
            fail "unknown argument: $1"
            ;;
        esac
        shift
      done
      ;;
    *)
      fail "unknown mode: $MODE"
      ;;
  esac
}

main() {
  parse_args "$@"

  if [[ "$MODE" == "release" ]]; then
    validate_version "$TARGET_VERSION"
    prepare_release_version "$TARGET_VERSION" 0
    assert_release_version_available "$TARGET_VERSION"
    commit_release_version_bump "$TARGET_VERSION"
  fi

  if [[ "$MODE" == "prepare" ]]; then
    prepare_release_version "$TARGET_VERSION"
    return
  fi

  local version
  if [[ "$MODE" == "release" ]]; then
    version="$TARGET_VERSION"
  else
    version="$(workspace_version)"
  fi
  [[ -n "$version" ]] || fail "failed to determine release version from Cargo.toml"

  local tag_name="v$version"

  run_pre_release_checks "$version" "$tag_name"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry run completed; skipping publish, tag creation, and push"
    log "note: dependent crates cannot be fully preflighted against crates.io until sibling crates are published"
    return
  fi

  publish_crates "$version"
  create_and_push_tag "$tag_name"
  log "release $tag_name completed"
}

main "$@"
