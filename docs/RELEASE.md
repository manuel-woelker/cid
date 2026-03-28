# What is this document for?

This document describes the intended release flow for `cid`.
Use it when publishing a crates.io release or shipping GitHub release artifacts.

# What does the release flow do?

The release flow has two parts:

- `./scripts/release.sh` prepares or validates the release version
- `./scripts/build-release.sh` builds a self-contained `cid` binary with the web UI appended as SquashFS
- `.github/workflows/release.yml` builds that packaged binary and attaches artifacts to a GitHub release when a `v*` tag is pushed

# What must be true before a release?

Before cutting a release:

- the worktree should be clean
- the workspace should build and test cleanly
- the CLI crate and any publishable internal crates should share the intended version
- the selected tag should not already exist

# How should the first release flow stay scoped?

Keep the early release flow deliberately small:

- one shared workspace version
- one release binary for Linux first
- one self-contained binary artifact instead of sidecar UI files
- no elaborate multi-platform packaging until the binary is actually useful

Premature release infrastructure is a great way to waste time on decorative nonsense.

# What should happen if a release fails halfway through?

Do not pretend a partial release did not happen.

Instead:

- inspect what was published successfully
- fix the root cause
- bump to a new version
- retry cleanly

That is less painful than trying to resurrect a broken half-release state.
