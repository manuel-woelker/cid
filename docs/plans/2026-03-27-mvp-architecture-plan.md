# What is this plan for?

This plan defines the first concrete implementation path for `cid` after the high-level architecture document.
The goal is to turn the broad system outline into a small set of crates, runtime responsibilities, and implementation steps that support a useful first version.

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
- `crates/cli`
- `crates/daemon`
- `crates/web`

Responsibilities:

- `crates/base`
  Shared utility types, error handling, logging, and other low-level helpers already started in the repository.
- `crates/cli`
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

The recommended first storage shape is:

- an embedded database for structured state
- filesystem-backed logs and retained artifacts

The database should hold:

- repositories
- branch rules
- discovered commits
- runs
- run steps
- lightweight aggregate statistics

The filesystem should hold:

- per-step logs
- retained artifacts
- optionally exported run summaries

This split is practical.
Trying to force logs into SQL from day one is unnecessary pain.

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

# What are the main risks?

The main risks are:

- Git change detection being more ambiguous than expected in real local workflows
- the scheduler creating duplicate or stale runs for fast-moving branches
- Docker invocation details varying more than expected across developer machines
- persistence growing ad hoc because the initial data model is too vague
- the UI demanding live-state behavior that the backend has not modeled cleanly

These are the places where disciplined boundaries will matter most.

# How should this work be tracked?

- [ ] Add `crates/daemon` with the initial core domain model
- [ ] Move daemon-facing state types out of the CLI crate
- [ ] Define repository registration and pipeline configuration types
- [ ] Define run, run-step, run-event, and run-status types
- [ ] Add a persistence layer for repositories and runs
- [ ] Implement a first repository watcher for tracked branches
- [ ] Implement a scheduler that turns discovered commits into queued runs
- [ ] Implement a sequential Docker runner for queued runs
- [ ] Persist step logs and run summaries during execution
- [ ] Add a minimal `crates/web` server with repository and run endpoints
- [ ] Add a minimal dashboard backed by those endpoints
- [ ] Add colocated tests for scheduling, persistence, and runner behavior
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- unit tests for scheduling decisions
- unit or integration tests for persistence round-trips
- focused tests for Docker command construction
- end-to-end tests for run-state transitions where practical
- repository-wide verification through `./scripts/check-code.sh`

If Docker-backed integration tests are too expensive for the early stages, keep the runner boundary narrow enough that command construction and run-state transitions can be tested separately.
