# What is this plan for?

This plan defines the first concrete implementation path for `cid` after the high-level architecture document.
The goal is to turn the broad system outline into a small set of crates, runtime responsibilities, and implementation steps that support a useful first version.

# What is the current status?

This plan is complete.

The core runtime shape is in place:

- `crates/base`, `crates/server`, `crates/daemon`, and `crates/web` exist
- the daemon watches repositories, schedules discovered commits, runs sequential Docker steps, persists run state, and exposes a minimal web API
- the server binary is now a thin process entrypoint, with the long-running daemon loop moved into `crates/daemon`
- the web UI exists and is backed by the runtime state

Two deliberate deviations remain:

- the server entrypoint does not yet implement repository registration or status-inspection commands beyond starting the daemon
- structured state is persisted as YAML plus filesystem logs rather than in an embedded database

# What problem is this plan solving?

`cid` now has product direction and a broad architecture description, but it does not yet have an implementation plan that answers practical questions such as:

- which crates should exist first
- what state must be persisted
- what boundaries should be explicit from the start
- what should be deferred until later

Without that plan, the codebase is likely to drift into one of two bad outcomes:

- everything lands in the CLI crate because it is the only runtime entrypoint
- speculative abstractions appear too early in anticipation of a much larger future system

Both outcomes are mediocre.

# What should the first version of the architecture optimize for?

The first version should optimize for:

- a daemon that can watch at least one repository
- deterministic scheduling for newly discovered commits
- reproducible Docker execution for configured steps
- persisted run history that survives restarts
- a minimal API surface for status and run inspection

The first version should not optimize for:

- distributed execution
- plugin systems
- multiple persistence backends
- real-time streaming protocols beyond what a simple local API needs
- a huge configuration language

# What crate layout should the MVP use?

The recommended initial workspace layout is:

- `crates/base`
- `crates/server`
- `crates/daemon`
- `crates/web`

Responsibilities:

- `crates/base`
  Shared utility types, error handling, logging, and other low-level helpers already started in the repository.
- `crates/server`
  Thin process entrypoint and command parsing for starting the daemon, registering repositories, and inspecting status.
- `crates/daemon`
  Core runtime logic: repository registry, Git watcher, scheduler, queue, runner integration, and persistence.
- `crates/web`
  Local HTTP server for status endpoints and the web UI asset serving layer.

This keeps the main runtime logic out of the CLI and avoids prematurely splitting into too many crates.

# What runtime boundaries should be explicit from the start?

The following boundaries should be explicit early:

- repository registry versus Git watcher
- Git watcher versus scheduler
- scheduler versus runner
- runner versus persistence
- persistence versus presentation

Those boundaries matter more than perfect crate purity.
Even if some modules live in the same crate at first, the interfaces should stay conceptually separate.

# What data model should the MVP use?

The MVP should define a small, explicit core model for:

- repository
- watched branch rule
- discovered commit
- run
- run step
- run event
- run status

Useful initial fields include:

- repository id, name, path
- branch name
- commit SHA
- queued, started, and finished timestamps
- Docker image name
- step name, command, exit status, and duration
- run status values such as `queued`, `running`, `passed`, `failed`, and `canceled`

The model should be designed for boring persistence and API serialization, not clever internal elegance.

# What storage approach should the MVP use?

The implemented MVP storage shape is:

- YAML-backed structured state under the daemon state directory
- filesystem-backed logs and retained artifacts

The structured state file currently holds:

- repositories
- discovered commits
- runs
- run steps
- lightweight summary-friendly data derived from stored runs

The filesystem holds:

- per-step logs
- retained artifacts

This is a deliberate simplification from the original embedded-database idea.
It keeps the first version boring and inspectable, but it should be revisited if state growth or query complexity starts to hurt.

# What execution flow should the MVP implement?

The intended end-to-end flow is:

1. the CLI or config layer registers a repository and its pipeline definition
2. the daemon starts and loads repository state from disk
3. the Git watcher polls or watches tracked repositories for ref changes
4. when a tracked branch points to a new commit, the watcher emits a discovery event
5. the scheduler checks whether that commit already has a run and, if not, enqueues one
6. the runner claims queued runs and executes them in Docker
7. logs, step results, and run metadata are persisted incrementally
8. the web API serves repository, run, and summary views from persisted state

For the MVP, polling is acceptable if it is reliable and simple.
Hook-based or filesystem-watch-heavy designs can come later if polling proves insufficient.

# What should the Docker runner support first?

The first runner should support only the boring path:

- one configured image per repository pipeline
- one repository workspace mounted into the container
- sequential step execution
- stdout and stderr capture
- exit code capture
- simple artifact path retention

Avoid adding:

- matrix builds
- parallel step graphs inside a single run
- multiple container services per job
- dynamic image-building logic inside the runner

Those features may become useful later, but they are absolutely not required to validate the core product.

# What web surface should the MVP expose?

The MVP API and UI should stay intentionally small.

Recommended initial capabilities:

- list repositories
- show repository status
- list recent runs
- show run detail and per-step status
- return basic summary statistics such as pass rate and recent durations

The UI can be minimal as long as it is genuinely useful.
A plain but clear local dashboard is better than an elaborate frontend with no reliable backend shape.

# In what order should the implementation happen?

Recommended order:

1. define the core daemon data model in `crates/daemon`
2. implement repository registration and persistence
3. implement simple commit detection for one repository
4. implement the run queue and basic scheduler
5. implement sequential Docker step execution
6. persist run state and logs incrementally
7. add a minimal web API for repositories and runs
8. add a basic web UI using those endpoints
9. expand to multi-repository support and aggregate statistics

This order keeps the critical path on the daemon instead of spending early effort on shell polish or frontend cosmetics.

# What assumptions should be made explicit?

This plan assumes:

- Docker is available on the local machine
- a polling-based repository watcher is good enough for the first version
- a single-machine local daemon is the only supported runtime model initially
- repository pipeline configuration can stay simple and static
- the early web UI will tolerate page-refresh-style interaction if needed

If any of those assumptions become false, the plan should be updated rather than silently stretched.

# What follow-up work is still outside this completed plan?

The most obvious follow-up work beyond this plan is:

- add server commands for repository registration and status inspection if the product still wants them at the process entrypoint
- revisit the persistence format if YAML state becomes awkward for upgrades, querying, or larger run histories

# What are the main risks?

The main risks are:

- Git change detection being more ambiguous than expected in real local workflows
- the scheduler creating duplicate or stale runs for fast-moving branches
- Docker invocation details varying more than expected across developer machines
- persistence growing ad hoc because the initial data model is too vague
- the UI demanding live-state behavior that the backend has not modeled cleanly

These are the places where disciplined boundaries will matter most.

# How should this work be tracked?

- [x] Add `crates/daemon` with the initial core domain model
- [x] Move daemon-facing state types out of the CLI crate
- [x] Define repository registration and pipeline configuration types
- [x] Define run, run-step, run-event, and run-status types
- [x] Add a persistence layer for repositories and runs
- [x] Implement a first repository watcher for tracked branches
- [x] Implement a scheduler that turns discovered commits into queued runs
- [x] Implement a sequential Docker runner for queued runs
- [x] Persist step logs and run summaries during execution
- [x] Add a minimal `crates/web` server with repository and run endpoints
- [x] Add a minimal dashboard backed by those endpoints
- [x] Add colocated tests for scheduling, persistence, and runner behavior
- [x] Run `./scripts/check-code.sh`
- [x] Do not add server commands for repository registration and status inspection beyond daemon startup in this MVP plan
- [x] Do not replace YAML-backed structured state with an embedded database in this MVP plan

These two items are intentionally closed as won't-fix for this plan.
They remain valid future product questions, but they are not required for the completed MVP architecture slice captured here.

# How should the work be verified?

Verification should include:

- unit tests for scheduling decisions
- unit or integration tests for persistence round-trips
- focused tests for Docker command construction
- end-to-end tests for run-state transitions where practical
- repository-wide verification through `./scripts/check-code.sh`

If Docker-backed integration tests are too expensive for the early stages, keep the runner boundary narrow enough that command construction and run-state transitions can be tested separately.
