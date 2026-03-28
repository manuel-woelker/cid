# What is this document for?

This document defines the intended persistence layout for `cid`.
It explains which data belongs in the central state store, which data belongs to individual repositories, and how the on-disk directory structure should be organized.

# Why split persistence into central and per-repository storage?

`cid` needs two different kinds of durable state:

- a small amount of global registry state
- repository-local state that is naturally scoped to a single repository

Trying to force everything into one database would make repository-local data harder to isolate and clean up.
Trying to store the repository registry inside each repository would make daemon startup and repository management awkward for no real benefit.

The simplest useful split is:

- one central SQLite database for the repository registry
- one repository directory per tracked repository
- one repository-local SQLite database inside each repository directory
- filesystem directories for workspaces, caches, logs, and artifacts

# What should the top-level persistence layout look like?

The root state directory should be `.cid`.

Recommended layout:

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

`<repository-key>` should be a stable filesystem-safe identifier for the repository.
It can be based on the repository id, a slugified name, or a combination of both.
The important part is that it stays stable across restarts and renames do not silently orphan repository data.

# What should be stored in `.cid/cid.db`?

`.cid/cid.db` is the central SQLite database for the repository registry.

It should store only repository registration data, such as:

- repository registry records
- repository identity, names, configured paths, and lifecycle metadata

This database is the source of truth for which repositories `cid` knows about.
All run history, discovered commits, tracked refs, and execution metadata should live in the repository-local database instead.

# What tables should `.cid/cid.db` contain?

The central database should stay tiny and boring.

Recommended tables:

- `repositories`

# What columns should the `repositories` table contain?

Recommended columns:

- `id INTEGER PRIMARY KEY`
- `repository_key TEXT NOT NULL UNIQUE`
- `name TEXT NOT NULL`
- `path TEXT NOT NULL`
- `status TEXT NOT NULL`
- `last_seen_at_ms INTEGER`
- `last_error TEXT`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

This table is the canonical repository registry.
`repository_key` should match the directory name used under `.cid/repositories/`.

# What should be stored in `.cid/repositories/<repository-key>/cid-repo.db`?

Each repository should have its own SQLite database at:

```text
.cid/repositories/<repository-key>/cid-repo.db
```

This database should store repository-local persistent state, such as:

- repository-specific run history
- repository-defined tracked refs
- repository-defined pipeline configuration
- enough run metadata to locate logs and artifacts on disk

This keeps high-volume repository-local state from bloating the central database.
It also makes repository deletion, export, backup, and repair less painful.

# What tables should `.cid/repositories/<repository-key>/cid-repo.db` contain?

The repository-local database should store execution history and metadata for one repository.

Recommended tables:

- `repo_state`
- `tracked_refs`
- `runs`

# What columns should the `repo_state` table contain?

Recommended columns:

- `repository_id INTEGER PRIMARY KEY`
- `repository_key TEXT NOT NULL UNIQUE`
- `name TEXT NOT NULL`
- `source_path TEXT NOT NULL`
- `workspace_path TEXT NOT NULL`
- `cache_path TEXT NOT NULL`
- `runs_path TEXT NOT NULL`
- `config_revision TEXT`
- `config_payload TEXT`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

This table should have exactly one row per repository-local database.
It makes the local database self-describing and easier to inspect or repair in isolation.
`config_payload` should store the upstream-repository-derived execution configuration in one boring serialized blob instead of spreading it across more tables.

# What columns should the `tracked_refs` table contain?

Recommended columns:

- `id INTEGER PRIMARY KEY`
- `ref_name TEXT NOT NULL UNIQUE`
- `commit_sha TEXT`
- `last_seen_at_ms INTEGER`
- `updated_at_ms INTEGER NOT NULL`

This table stores which refs the repository-local executor currently tracks.
These refs should be derived from the upstream repository rather than from daemon-level config.

# What columns should the `runs` table contain?

Recommended columns:

- `id INTEGER PRIMARY KEY`
- `ref_name TEXT NOT NULL`
- `commit_sha TEXT NOT NULL`
- `status TEXT NOT NULL`
- `queued_at_ms INTEGER NOT NULL`
- `started_at_ms INTEGER`
- `finished_at_ms INTEGER`
- `duration_ms INTEGER`
- `run_dir TEXT NOT NULL`
- `workspace_revision TEXT`
- `step_results_json TEXT`
- `events_json TEXT`
- `artifacts_json TEXT`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

`run_dir` should point at the matching directory under `runs/`.
`step_results_json`, `events_json`, and `artifacts_json` intentionally keep the schema compact.
If this ever becomes painful, the model can be normalized later, but the default should be to stay boring.

# What is the `workspace/` directory for?

Each repository directory should include:

```text
.cid/repositories/<repository-key>/workspace/
```

This directory is the working area used to perform builds.

It should be used for:

- materializing or syncing the repository checkout used for execution
- temporary build-time files that belong to the repository execution context
- mount points or working directories used by Docker runs

This directory is operational state, not the long-term system of record.
It may be recreated or refreshed as needed.

# What is the `cache/` directory for?

Each repository directory should include:

```text
.cid/repositories/<repository-key>/cache/
```

This directory is for persistent repository-local cache data.

It should be used for:

- build caches that are safe to retain across runs
- dependency caches
- tool caches that materially improve repeat execution time

Caches are persistent, but they are not authoritative state.
`cid` should be able to clear or rebuild them without corrupting the repository record.

# What is the `runs/` directory for?

Each repository directory should include:

```text
.cid/repositories/<repository-key>/runs/
```

This directory stores run outputs that are better kept as files than as database rows.

It should be used for:

- per-step logs
- retained artifacts
- exported reports or other opaque run outputs

Append-heavy log data and arbitrary artifact files do not belong in SQLite unless there is a very good reason.
The filesystem is the simpler and more durable fit here.

# How should the central database and repository databases relate to each other?

The split should stay boring:

- the central database owns global identity and coordination
- the repository database owns repository-local execution history and metadata
- the filesystem owns large or append-heavy outputs

In particular:

- the central database should contain only the repository registry
- run history should live in the repository-local database
- repository-specific ref and pipeline definitions should live in the repository-local database
- repository-local execution config should be derived from the upstream repository itself

The central database may store references into repository-local databases or files, but it should not duplicate large repository-local payloads just to keep everything in one place.

# What should be avoided?

Avoid these persistence mistakes:

- putting large step logs into SQLite blobs by default
- treating cache contents as authoritative state
- letting repository directory names drift without a stable identifier strategy
- duplicating the same high-volume run data in both the central and repository-local databases
- mixing temporary execution scratch space with durable run outputs

# What should future implementation work preserve?

Any implementation should preserve these invariants:

- `.cid/cid.db` exists as the central repository-registry SQLite database
- `.cid/repositories/` contains one directory per tracked repository
- every repository directory contains `cid-repo.db`, `workspace/`, `cache/`, and `runs/`
- `.cid/cid.db` contains only the `repositories` table
- repository-specific execution config lives in the repository-local database, not in `.cid/cid.db`
- each repository-local database contains only `repo_state`, `tracked_refs`, and `runs`
- logs and artifacts are stored as files under the repository directory, not shoved into the central database
- repository-local execution history does not leak into the central database
