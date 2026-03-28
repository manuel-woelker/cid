# What is this plan for?

This plan defines how to implement the persistence model documented in [PERSISTENCE.md](/data/projects/cid/docs/PERSISTENCE.md).
The goal is to replace the current YAML-based daemon state with the documented SQLite-plus-filesystem layout without smuggling legacy compatibility work into unrelated runtime changes.

# What problem is this plan solving?

The repository now documents a persistence model with:

- a central repository-registry database at `.cid/cid.db`
- one repository directory per tracked repository under `.cid/repositories/`
- one repository-local SQLite database at `.cid/repositories/<repository-key>/cid-repo.db`
- repository-local `workspace/`, `cache/`, and `runs/` directories

The implementation does not match that model yet.
Today, [`crates/daemon/src/persistence.rs`](/data/projects/cid/crates/daemon/src/persistence.rs) still persists daemon state into `state.yaml` and writes logs into a flat `logs/` directory under the state root.

That gap is now big enough to be annoying:

- the docs and implementation disagree
- the current persistence boundary is too coarse
- repository-local lifecycle management is harder than it needs to be
- the future SQLite migration will only get messier if the YAML layout keeps growing

# What should the implementation optimize for?

This implementation should optimize for:

- a boring migration from YAML state to SQLite-backed storage
- an explicit split between central registry state and repository-local state
- straightforward filesystem layout creation
- small, testable persistence APIs
- minimal churn outside persistence and repository/runtime wiring

It should not optimize for:

- a generalized ORM layer
- dynamic schema builders
- multiple persistence backends
- speculative query caching
- perfect one-shot data migration for every imagined future format

# What is the target on-disk layout?

The implementation target is:

```text
.cid/
  cid.db
  repositories/
    <repository-key>/
      cid-repo.db
      workspace/
      cache/
      runs/
```

The central database should contain only the `repositories` table.
Each repository-local database should contain only:

- `repo_state`
- `tracked_refs`
- `runs`

Logs and artifacts should remain on disk under the repository-local `runs/` directory.

# What should the central database store?

The central database at `.cid/cid.db` should own only repository registration data.

Initial `repositories` table shape should match [PERSISTENCE.md](/data/projects/cid/docs/PERSISTENCE.md):

- `id INTEGER PRIMARY KEY`
- `repository_key TEXT NOT NULL UNIQUE`
- `name TEXT NOT NULL`
- `path TEXT NOT NULL`
- `status TEXT NOT NULL`
- `last_seen_at_ms INTEGER`
- `last_error TEXT`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

This database should not store:

- discovered commits
- tracked refs
- pipeline configuration
- run history
- summary counters

# What should each repository-local database store?

Each repository-local database should store:

- `repo_state`
  One self-describing row with repository identity, derived config payload, and local paths.
- `tracked_refs`
  The refs currently tracked for that repository and the last observed commit for each ref.
- `runs`
  The repository’s run history, including JSON blobs for step results, events, and artifact metadata.

The persistence API should treat the repository-local database as the source of truth for repository execution history.

# How should the current YAML state be handled?

The current code persists one `DaemonState` YAML blob.
That format should be removed rather than migrated.

This plan assumes a clean cutover:

1. introduce SQLite-backed stores and filesystem layout helpers
2. switch daemon load/save behavior to the new layout
3. stop reading and writing `state.yaml`
4. remove legacy YAML persistence code

Preserving old local state is explicitly out of scope for this implementation.
That keeps the persistence rewrite smaller and avoids spending effort on compatibility code that the project does not need.

# What code boundaries should be introduced?

The persistence code should stop pretending one store object can own every kind of state cleanly.

Recommended boundaries:

- a central registry store for `.cid/cid.db`
- a repository store for one `.cid/repositories/<repository-key>/cid-repo.db`
- a layout helper for repository directory creation and path resolution
- a migration helper for importing legacy YAML state

That does not require a deep abstraction stack.
It does require splitting responsibilities so tests can verify each piece independently.

# How should repository keys and directories be handled?

Repository keys must be stable and filesystem-safe.

The implementation should choose one deterministic strategy and stick to it, for example:

- a slugified repository name plus the numeric repository id
- or a purely numeric id if human readability is not worth the edge cases

The important constraints are:

- the key is stable across restarts
- different repositories cannot collide
- renaming a repository does not silently orphan its directory

# How should logs and artifacts move to repository-local directories?

The current log path shape is roughly:

```text
.cid/logs/run-<id>/step-<index>.log
```

The target shape should be repository-local, for example:

```text
.cid/repositories/<repository-key>/runs/run-<id>/step-<index>.log
```

Artifacts should follow the same repository-local convention.
The `runs` table should store enough metadata to locate those files without normalizing them into separate tables.

# In what order should the implementation happen?

Recommended order:

1. add SQLite dependencies and low-level connection helpers
2. implement central registry schema creation for `.cid/cid.db`
3. implement repository directory layout creation under `.cid/repositories/`
4. implement repository-local schema creation for `repo_state`, `tracked_refs`, and `runs`
5. add read/write APIs for the central `repositories` table
6. add read/write APIs for repository-local run and ref data
7. update log-writing paths to use repository-local `runs/` directories
8. switch daemon load/save logic to the SQLite-backed stores
9. remove legacy YAML persistence code
10. run repository-wide verification and manual smoke checks

This order keeps the migration incremental and reduces the chance of breaking restart behavior halfway through.

# What assumptions should be made explicit?

This plan assumes:

- SQLite will be the only structured persistence backend
- repository-specific execution config can be stored as a serialized blob in `repo_state.config_payload`
- step results, events, and artifact metadata can live as JSON blobs in the `runs` table for now
- repository count will stay modest enough that loading some repository-local state on demand is acceptable

If any of those assumptions become false, the implementation should be revised before the persistence APIs harden.

# What are the main risks?

The main risks are:

- unstable repository-key generation that causes orphaned directories
- pushing too much runtime logic into the persistence layer
- path mismatches between stored metadata and on-disk run directories
- under-testing restart behavior because the happy path compiles

Those are boring risks, which is good, but they still need explicit coverage.

# How should this work be tracked?

- [ ] Add the SQLite crate(s) needed for persistence
- [ ] Add central registry schema creation for `.cid/cid.db`
- [ ] Add repository directory layout creation under `.cid/repositories/`
- [ ] Add repository-local schema creation for `repo_state`, `tracked_refs`, and `runs`
- [ ] Add a stable repository-key generation strategy
- [ ] Add a central registry store API for repository rows
- [ ] Add a repository-local store API for repo state, tracked refs, and runs
- [ ] Move step-log writes into repository-local `runs/` directories
- [ ] Switch daemon load/save behavior to the new persistence stores
- [ ] Remove legacy YAML persistence code
- [ ] Add colocated tests for schema creation, repository layout creation, and persistence round-trips
- [ ] Add regression coverage for repository-local log-path generation
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- unit tests for repository-key generation
- unit tests for schema creation and idempotent layout initialization
- persistence round-trip tests for the central `repositories` table
- persistence round-trip tests for repository-local `repo_state`, `tracked_refs`, and `runs`
- focused tests for repository-local log-path generation
- repository-wide verification through `./scripts/check-code.sh`

Manual smoke checking is also worth doing once the implementation lands:

- start the daemon with an empty `.cid` directory
- confirm repository directories and databases are created in the documented locations
- confirm new run logs land under the repository-local `runs/` directory
