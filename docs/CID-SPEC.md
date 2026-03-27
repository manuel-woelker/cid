# What is this document?

This document sketches the intended local storage model for `cid`.
It focuses on the files and directories that should exist so builds, reports, and statistics remain inspectable after execution.

This is a design guide, not a frozen specification.

# Where should `cid` store its local state?

`cid` should keep its local state under a dedicated repository-owned directory.

The likely default is:

```text
.cid
```

That keeps build metadata close to the repository while remaining easy to ignore in Git.

# What should live under `.cid`?

A useful first layout would look like this:

```text
.cid/
  repos/
  runs/
  artifacts/
  stats/
```

The exact structure may evolve, but the intent should stay stable:

- `repos/` stores watched-repository metadata
- `runs/` stores individual build records
- `artifacts/` stores retained outputs referenced by runs
- `stats/` stores aggregate data used by the dashboard

# How should run directories be named?

Each run directory should include:

- a filesystem-safe timestamp
- a repository identifier
- a commit identifier or abbreviated commit SHA

An example:

```text
2026-03-27T21-14-08Z-cid-a3beaff
```

That preserves sort order while keeping the run origin obvious.

# Which files should a run directory contain?

Each run directory should contain enough information to explain both intent and outcome.

At a minimum:

- `run.json`
- `events.jsonl`
- one log file per pipeline step
- `summary.json`

# What should `run.json` contain?

`run.json` should describe the build before execution starts.

At a minimum:

- repository identity and path
- branch and commit SHA
- selected pipeline configuration
- Docker image and execution details
- queued timestamp

# What should `events.jsonl` contain?

`events.jsonl` should record the chronological lifecycle of the run.

Likely event types:

- run queued
- run started
- step started
- step finished
- run finished

This file should be append-only so the UI can follow an in-progress run incrementally.

# How should step logs work?

Each pipeline step should have its own log file.
That keeps output readable and makes it easier for the UI to display failures without interleaving unrelated noise.

The implementation may also store combined output for convenience, but per-step logs should remain the primary source of truth.

# What should `summary.json` contain?

`summary.json` should be the stable final record for a completed run.

At a minimum:

- overall run status
- started and finished timestamps
- total duration
- one entry per step
- exit status or failure message where relevant
- retained artifact references

# What should statistics storage optimize for?

Statistics storage should optimize for cheap reads in the local dashboard, not for academic perfection.

Useful aggregates include:

- pass rate by repository
- median and p95 duration by step
- most common failing steps
- recent throughput

If a lightweight embedded database handles that cleanly, use one.
