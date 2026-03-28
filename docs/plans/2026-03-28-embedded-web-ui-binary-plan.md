# What is this plan for?

This plan defines how to ship the web UI from a self-contained `cid` binary.
The goal is to package `ui/dist` into a SquashFS image during release, append that image to the final executable with a small trailer, and teach the web server to serve non-`/api/` requests from the embedded filesystem through `backhand`.

# What problem is this plan solving?

Today [`crates/web/src/lib.rs`](/data/projects/cid/crates/web/src/lib.rs) serves frontend assets directly from `ui/dist` on the host filesystem.
That is fine for local development, but it is the wrong deployment shape for a single-file release binary:

- the binary is not self-contained
- release artifacts depend on an external `ui/dist` directory being present
- packaging and startup behavior differ more than they need to
- the current release script does not know anything about frontend assets

If the project wants a single binary that can be copied anywhere and still serve the UI, the asset boundary has to move from "files next to the executable" to "files inside the executable."

# What is the current status?

The repository already has the pieces that make this feature worth doing:

- `ui` builds a production `dist` directory
- `crates/web` already distinguishes `/api/*` from frontend asset requests
- the server entrypoint is the `cid` binary in [`crates/server`](/data/projects/cid/crates/server)

What is missing is the embedding pipeline and the runtime reader:

- [`scripts/release.sh`](/data/projects/cid/scripts/release.sh) does not build or package the UI
- the final executable has no embedded asset trailer
- `crates/web` only knows how to read assets from the host filesystem

# What should the implementation optimize for?

This work should optimize for:

- a boring single-binary release artifact
- zero extraction of UI assets to temporary directories at runtime
- one-time parsing of the embedded filesystem, not per-request work
- clear failure modes when the embedded payload is missing or malformed
- minimal disruption to local frontend development

This work should not optimize for:

- hot-reloading from the embedded image
- multiple embedded asset packs
- a custom archive format when SquashFS already exists
- runtime mutation of frontend assets

# What binary layout should be written by the release flow?

The release flow should produce the final executable in this order:

1. build `ui/dist`
2. pack `ui/dist` into a SquashFS image
3. build the release `cid` binary
4. append the SquashFS bytes to the binary
5. append the SquashFS size as one 32-bit integer
6. append the ASCII magic string `SQUASHFS`

That trailer gives the runtime enough information to find the embedded payload by reading backward from the end of the executable.

The implementation should make two details explicit instead of leaving them implicit:

- the 32-bit size encoding and endianness
- whether the stored size is limited to `u32`, which caps the embedded image at 4 GiB

Little-endian `u32` is the sensible default unless a different requirement exists.

# How should the runtime locate the embedded SquashFS image?

The server should derive the executable path from `std::env::current_exe()`, open that file, and inspect the final bytes.

Recommended read flow:

1. read the last 8 bytes and verify they equal `SQUASHFS`
2. read the preceding 4 bytes as the embedded image length
3. compute the SquashFS start offset from `file_len - 8 - 4 - squashfs_len`
4. reject the payload if the offsets underflow or point outside the executable
5. expose a reader over just that byte range to `backhand`

The parser should live in a small dedicated module rather than being folded into request routing.
That keeps the format logic testable and avoids contaminating the HTTP code with binary-offset arithmetic.

# How should the web server serve embedded assets?

`crates/web` should stop treating frontend assets as "always on disk under `ui/dist`."
Instead it should support two asset sources:

- embedded SquashFS assets for packaged binaries
- host filesystem assets from `ui/dist` when no embedded payload is present

That fallback matters because local development and tests should not depend on the release-packaging path.

Recommended runtime shape:

- introduce an asset-source type owned by `crates/web`
- load the asset source once during `serve(...)` startup
- keep API routing exactly as it is for `/api/*`
- route every other request through the asset source
- preserve SPA fallback to `index.html` for non-file frontend routes

The server should not reopen and reparse the executable on every request.
Load once, share it for the life of the process, and keep the request path boring.

# What code boundaries should be introduced?

Recommended boundaries:

- a packaging helper in [`scripts/release.sh`](/data/projects/cid/scripts/release.sh) for building `ui/dist`, producing the SquashFS image, and appending the trailer
- an embedded-ui module in `crates/web` that parses the executable trailer and opens the SquashFS image through `backhand`
- an asset-source abstraction in `crates/web` that hides whether bytes come from the embedded image or the filesystem
- focused content-type and SPA-fallback helpers that stay independent of the storage backend

This does not need a deep abstraction stack.
It does need one clean seam between HTTP routing and asset lookup.

# How should the release script change?

[`scripts/release.sh`](/data/projects/cid/scripts/release.sh) should grow explicit UI packaging steps before the final release artifact is considered complete.

Recommended changes:

- require the SquashFS tooling needed to build the image
- run `pnpm --dir ui build`
- create a SquashFS image from `ui/dist`
- build the release `cid` binary
- append the SquashFS image, size trailer, and `SQUASHFS` magic to that binary
- verify that the resulting binary still executes and that the trailer can be read back

The script should fail loudly if the UI build or SquashFS creation fails.
Silent fallback to an unembedded release binary would be a nasty footgun.

# How should development and non-release behavior work?

Development should remain boring:

- `pnpm --dir ui dev` still serves the frontend during UI work
- local Rust runs can keep serving `ui/dist` from disk when present
- tests in `crates/web` can continue using simple filesystem-backed fixtures where that is easier

The embedded path should be the production path, not the mandatory path for every local command.

# What assumptions should be made explicit?

This plan assumes:

- the final release artifact is the `cid` binary from [`crates/server`](/data/projects/cid/crates/server)
- `backhand` can read a SquashFS image from a byte range or reader without requiring full extraction
- serving assets directly from the embedded image is fast enough for the expected UI size
- the UI artifact is comfortably smaller than the 4 GiB limit implied by a 32-bit length trailer
- local development should keep working without running the release script

If `backhand` turns out to require whole-image extraction or a materially different I/O shape, the implementation should be revised before the embedding format hardens.

# What are the main risks?

The main risks are:

- encoding the trailer ambiguously and making future binaries unreadable
- parsing the embedded payload on every request and turning asset serving into needless I/O churn
- breaking local development by removing the filesystem fallback too early
- mishandling SPA fallback inside the SquashFS reader and regressing client-side routes
- producing release binaries that pass build checks but do not actually serve the UI

# In what order should the implementation happen?

Recommended order:

1. add the `backhand` dependency and a small embedded-ui parser module in `crates/web`
2. define and test the executable trailer format reader independently of HTTP serving
3. add an asset-source type that can read from either embedded SquashFS or `ui/dist`
4. switch non-`/api/` routing in `crates/web` to use the new asset source
5. keep SPA fallback and content-type behavior consistent across both asset sources
6. teach `scripts/release.sh` to build the UI, create the SquashFS image, and append it to the release binary
7. add an end-to-end verification step that confirms the packaged binary serves `/` without `ui/dist` on disk

This order keeps the trailer format and runtime loader testable before the release script starts mutating binaries.

# How should this work be tracked?

- [ ] Add `backhand` to the relevant crate dependencies
- [ ] Add a small `crates/web` module for reading the appended `SQUASHFS` trailer from the current executable
- [ ] Make the trailer reader validate magic, size, and bounds defensively
- [ ] Add an asset-source type that supports embedded SquashFS assets and filesystem assets
- [ ] Switch non-`/api/` request handling in `crates/web` to read through the asset source
- [ ] Preserve SPA fallback to `index.html` for frontend routes served from the embedded image
- [ ] Preserve or improve content-type handling for embedded assets
- [ ] Keep a filesystem fallback for local development when no embedded payload is present
- [ ] Update `scripts/release.sh` to run `pnpm --dir ui build`
- [ ] Update `scripts/release.sh` to create a SquashFS image from `ui/dist`
- [ ] Update `scripts/release.sh` to append the image, 32-bit size trailer, and `SQUASHFS` magic to the release binary
- [ ] Add release-script validation that the packaged binary contains a readable trailer
- [ ] Add colocated unit tests for trailer parsing and bounds validation
- [ ] Add colocated tests for embedded asset lookup, content types, and SPA fallback behavior
- [ ] Add a smoke test or scripted verification that the packaged binary serves the UI without `ui/dist` on disk
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- unit tests for the trailer parser, including bad magic, truncated size, and out-of-bounds offsets
- unit tests for resolving embedded asset paths and SPA fallback behavior
- tests that keep `..` path rejection intact for embedded serving
- a release-packaging check that builds a binary with an appended SquashFS image and confirms the reader can open it
- a smoke check that starts the packaged binary after hiding or removing `ui/dist` and confirms `/` returns the built frontend
- repository-wide verification through `./scripts/check-code.sh`

# What improvements or follow-up work are worth considering?

Useful follow-up ideas, but outside the core implementation:

- log whether the server started with embedded assets or filesystem assets to make deployment debugging less annoying
- expose a small internal diagnostic for the active asset source if operations need it later
- decide whether release packaging should produce both a plain binary and a self-contained binary, or only the self-contained one
