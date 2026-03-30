# What is this document for?

This document outlines the broad architecture of `cid`.
It describes the main runtime components, how they interact, and what the first version should optimize for.

It is intentionally high level.
The goal is to make implementation direction obvious without freezing every internal boundary too early.

# What are the main parts of `cid`?

At a broad level, `cid` has six major pieces:

- a repository registry
- a Git watcher
- a scheduler
- a job runner
- a local state store
- a web API and UI

Those pieces support one continuous flow:

1. track local repositories
2. detect new commits
3. create build runs
4. execute those runs in Docker
5. persist logs and results
6. expose reports and statistics through the UI

# What does the high-level data flow look like?

```text
watched git repos
       |
       v
  repo registry
       |
       v
   git watcher
       |
       v
    scheduler
       |
       v
     run queue
       |
       v
   docker runner
       |
       +--> step logs
       +--> run metadata
       +--> artifacts
       +--> aggregate stats
                |
                v
           local store
                |
         +------+------+
         |             |
         v             v
      web API        CLI
         |
         v
       web UI
```

# What is the responsibility of the repository registry?

The repository registry is the source of truth for which repositories `cid` knows about.

It should store:

- repository name
- local filesystem path
- repository status metadata

The registry should stay boring.
If adding or updating a repository feels complicated, the product is already drifting.

Repository-specific refs and pipeline configuration should not live in the central registry.
That data should come from the upstream repository itself and be cached per repository when needed.

# What is the responsibility of the Git watcher?

The Git watcher observes registered repositories and detects when tracked refs advance to a new commit.

Its job is not to run builds directly.
Its job is to emit events like:

- repository added
- repository unavailable
- ref advanced
- commit discovered

The watcher should be resilient to normal local-machine mess:

- repositories temporarily disappearing
- branch checkouts changing
- force-push-like local ref rewrites
- editors and tools briefly locking files

# What is the responsibility of the scheduler?

The scheduler decides what should run and when.

It consumes commit discovery events and turns them into build runs.
That includes decisions such as:

- should this commit be built?
- is there already a run for this commit?
- should older queued runs for the same branch be skipped?
- how many builds may run at once?

The first version should keep scheduling policy simple and predictable.
Fancy prioritization can come later if the basic queue is trustworthy.

# What is the responsibility of the run queue?

The run queue is the boundary between planning and execution.

It should represent runs in states such as:

- queued
- starting
- running
- passed
- failed
- canceled

Keeping the queue explicit matters because the UI and API both need a coherent view of what is happening right now, not just what happened in the past.

# What is the responsibility of the Docker runner?

The Docker runner executes build steps inside configured container environments.

Its job is to provide reproducible execution, not to be clever.
For a given run, it should:

- resolve the configured image
- mount the repository workspace
- set the working directory
- run each configured step in order
- capture stdout, stderr, exit status, and timing
- retain declared artifacts when relevant

The runner should produce a stable stream of events and logs so the rest of the system does not need to care whether the build is still live or already completed.

# What should be stored locally?

`cid` is local-first, so local storage is a core part of the architecture rather than an implementation detail.

The store should retain:

- registered repositories
- commit discovery history
- run records
- per-step logs
- artifact references
- timing and success/failure aggregates

The storage model should optimize for:

- inspectability
- straightforward recovery after restart
- cheap reads for the dashboard

# What persistence approach should `cid` use?

The default persistence model should be hybrid:

- SQLite for structured state
- the filesystem for logs and retained artifacts

The concrete layout and recommended table structure live in [PERSISTENCE.md](/data/projects/cid/docs/PERSISTENCE.md).

At a high level:

- `.cid/cid.db` stores only the repository registry
- `.cid/repositories/<repository-key>/cid-repo.db` stores repository-local execution history and repository-derived execution config
- repository-local `runs/` directories store logs and artifacts

This split keeps the storage model practical.
Trying to force logs and artifacts into the database from day one would add complexity without making the product better.

# How should the web API fit into the system?

The web API should expose the daemon state in a way that is easy for the UI to consume.

Likely API concerns:

- repository listing and detail views
- active and recent runs
- run detail with step logs
- summary statistics
- health and status endpoints

The API should read mostly from persisted local state instead of depending on fragile in-memory-only views.
That keeps restart behavior sane and avoids a weird split between "live" and "historical" data models.

For live runtime behavior, the daemon should still be the only mutable authority.
The web layer should be stateless presentation code:

- it should translate HTTP requests into daemon queries or commands
- it should not mutate persisted state directly
- it should not maintain its own mutable runtime model

That keeps the product's ownership model boring:

- the daemon owns live state
- persistence provides recovery and durability
- the web layer presents daemon-owned state

# How should the web UI fit into the system?

The UI is an operational surface, not decoration.

It should help the user answer:

- what is failing?
- what just changed?
- what is running?
- which repositories are unhealthy?
- where is time being spent?

That implies a few obvious views:

- repositories overview
- recent runs
- run detail
- branch and commit history
- statistics and trends

If the UI cannot make failures easier to understand than raw logs, it is not doing enough.

# Where should the CLI fit?

Even with a web UI, the CLI still matters.

It should provide a thin control surface for things like:

- starting the daemon
- registering repositories
- listing status
- triggering or retrying runs
- opening or printing useful diagnostic information

The CLI should not duplicate the daemon’s business logic.
It should be a client of the same core runtime and storage model.

# What internal boundaries matter most?

The important boundaries are:

- repository discovery versus scheduling
- scheduling versus execution
- execution versus persistence
- persistence versus presentation

One more boundary matters in practice:

- daemon authority versus presentation access

Presentation code should be able to ask questions and send commands, but it should not become a second writer of runtime state.

Those boundaries keep the system understandable.
They also make it possible to test the daemon without having every test depend on Docker, the web UI, and live Git state at the same time.

# How should the daemon runtime be split internally?

The daemon should stay the only mutable authority, but its internal runtime should be split by state transition rather than by broad component labels.

The useful stages are:

- repository/config sync
- commit discovery
- run planning
- run dispatch
- run execution
- snapshot publication

Those stages separate cheap control-plane work from slow external side effects.
That matters because repository polling, retries, and queue maintenance should stay responsive even while a build is running.

The most important handoff is:

- `run queued` means planning has created runnable work
- `run claimed for execution` means dispatch has handed that work to the executor

In practice that means:

- discovery may create queued runs, but it should not start builds directly
- any action that creates runnable queued work should wake dispatch immediately
- dispatch decides whether execution capacity is available
- execution performs the slow side effects and reports completion back to the daemon
- the daemon persists and republishes state after planning, dispatch, and execution transitions

This is intentionally not a generic event bus.
It is a pragmatic internal pipeline that keeps ownership obvious:

- discovery finds facts
- planning creates intended work
- dispatch decides what starts now
- execution performs the work
- publication makes the latest daemon-owned snapshot visible

# What should the first version avoid?

The first version should avoid architecture that assumes a future distributed system.

That means avoiding:

- remote worker coordination
- cross-machine locks
- provider-specific CI abstractions
- highly dynamic execution graphs
- overbuilt plugin systems

`cid` should first be a very good local daemon.
Everything else is follow-up work.

# What implementation shape is likely?

The most likely first implementation is a Rust workspace with separate crates for:

- shared base utilities
- daemon/runtime logic
- CLI
- web server
- UI frontend if it is built separately

That structure is not mandatory, but the conceptual split is useful even if the early implementation keeps several components together.
