# What is this plan for?

This plan defines how `cid` should evolve from a monolithic daemon cycle into an event-driven runtime split around state transitions.

The goal is to make repository polling, run queueing, retries, and execution responsive without giving up the daemon-authority model.

# What problem is this plan solving?

Today one daemon cycle does all of this in one synchronous pass:

1. sync repository definitions
2. poll Git for new commits
3. enqueue and cancel runs
4. execute queued runs
5. persist state
6. publish a snapshot

That design keeps the code simple, but it couples cheap control-plane work to slow execution work.

In practice, that creates visible product problems:

- a new commit may wait until the next poll interval before it is discovered
- a retried run may wait until the next poll interval before execution begins
- one slow build delays later queued runs from even being considered
- repository polling and queue maintenance are blocked by long-running build execution
- HTTP-triggered daemon commands are awkward because "do work now" shares the same path as "run the whole world"

This is not really a "repo discovery versus run execution" problem.
It is a state-transition boundary problem.

# What split is better than "repo discovery versus run discovery/execution"?

The better split is by event and responsibility.

Recommended event boundaries:

1. repository/config sync
2. commit discovery
3. run planning
4. run dispatch
5. run execution
6. snapshot publication

Those boundaries separate:

- pure state derivation
- scheduling decisions
- slow external side effects
- read-model publication

That is the split that actually matters for responsiveness.

# What state transitions should the runtime model explicitly?

The runtime should explicitly represent these transitions:

Repository/config sync:

- repository added
- repository removed
- repository config changed
- repository unavailable

Commit discovery:

- ref advanced
- commit discovered

Run planning:

- replay requested
- run queued
- queued run canceled

Run dispatch:

- run claimed for execution
- run started

Run execution:

- step started
- step finished
- run finished

Publication:

- snapshot published

The important handoff is:

- `run queued` means planning is done
- `run claimed for execution` means the executor has taken responsibility

That gives `cid` a real queue boundary instead of a fuzzy "maybe it will run later in this cycle" behavior.

# What runtime loops should `cid` have?

Recommended model: three daemon-owned internal loops.

1. Discovery loop

- sync repositories from config
- poll watched repositories for ref movement
- emit commit discovery events
- turn discovered commits into queued runs
- persist and publish the updated state

2. Dispatch loop

- watch for runnable queued runs
- enforce concurrency limits
- claim runs for execution
- hand claimed runs to the executor
- persist and publish the updated state

3. Execution loop

- perform slow side effects for claimed runs
- update step and run status as work progresses
- persist and publish the updated state

All three loops should still be daemon-owned.
The split is internal structure, not authority transfer.

# What should remain the single source of truth?

The daemon's owned runtime state should remain the single mutable authority.

This plan does not weaken the daemon-authority architecture.
It sharpens it:

- one authority
- multiple internal stages
- explicit state transitions

Persistence stays a daemon-owned durability layer.
The web layer stays stateless and dumb.

# How should immediate retry execution work under this model?

Retry should not wait for the next poll interval.

Recommended flow:

1. web layer sends `ReplayRun`
2. daemon validates and queues the replayed run immediately
3. daemon persists and publishes the queued state
4. dispatcher notices the new queued run immediately
5. dispatcher claims it for execution if capacity allows
6. executor starts it without waiting for repository polling

That is the product behavior users expect.

The key insight is that retry is a run-planning event, not a repository discovery event.

# How should repository polling fit after this split?

Repository polling should drive commit discovery only.

It should not be responsible for:

- deciding when queued runs start
- unblocking retries
- pacing execution

That means the poll interval affects how quickly new commits are discovered, but it should not affect how quickly already-queued work begins execution.

Any event that creates runnable queued work should wake dispatch immediately.

That includes:

- a newly discovered commit that queued a run
- a replay request that queued a run
- execution capacity becoming available after a run finishes

Discovery should not start execution directly.
Discovery should wake dispatch, and dispatch should decide whether execution can begin now.

# What should the dispatcher be responsible for?

The dispatcher should be the only place that decides:

- whether a queued run is runnable now
- how many runs may execute concurrently
- which queued run gets claimed next
- whether repository-local serialization is needed

This is where later policy should live:

- max parallel runs
- one-run-per-repository limits
- prioritizing replays
- deprioritizing superseded queued runs

That policy does not belong in the Git polling path and does not belong in the executor.

# What should the executor be responsible for?

The executor should only do slow external work:

- build devcontainers if needed
- start containers
- stream logs
- update step status
- mark the run finished

The executor should not:

- discover commits
- decide which run to start next
- mutate repository config state
- inspect every queued run in the system

That keeps the slow path narrow and easier to reason about.

# Should the runtime be event-driven or just split into more synchronous functions?

It should be event-driven enough to make ownership and wakeups explicit, but it does not need to become a distributed-systems science project.

Recommended implementation style:

- one daemon-owned runtime
- internal channels for commands and work notifications
- explicit wakeups between planning, dispatch, and execution
- bounded worker model for execution

Avoid overengineering:

- no generic event bus
- no plugin-grade subscription system
- no attempt to model every state transition as a public abstraction on day one

This should be a pragmatic internal pipeline, not an enterprise messaging platform.

# What data structures should be introduced?

Recommended additions:

- a `QueuedRun` or claimable run view is probably unnecessary if `RunStatus` remains authoritative
- a dispatcher queue or wake signal for newly queued work
- an execution claim marker so the dispatcher and executor do not race on the same queued run
- a daemon-owned concurrency limit configuration
- possibly a small internal event enum for runtime wakeups

A minimal internal event enum could look like:

- `RepositoriesMayNeedSync`
- `CommitsDiscovered`
- `RunsQueued`
- `ExecutionCapacityAvailable`
- `CommandReceived`

The important thing is not the enum itself.
The important thing is making wakeups explicit instead of hiding them inside a polling sleep.

# What code structure changes are needed?

Recommended refactor path:

1. separate the current `run_cycle()` into named phases
2. extract repository sync and commit discovery from execution
3. extract a dispatcher that claims queued runs independently of discovery
4. change the runner so it executes claimed work instead of scanning all queued runs
5. add immediate dispatcher wakeups when replay or commit queueing creates new queued runs
6. add execution-capacity wakeups when a run finishes
7. keep persistence and snapshot publication daemon-owned after each state transition

The first useful version does not need full parallel execution.
Even a single-worker dispatcher/executor split would materially improve responsiveness because queueing would no longer wait behind the next poll.

# What implementation order is recommended?

Recommended order:

1. introduce explicit phase helpers inside the daemon:
   - `sync_repositories`
   - `discover_commits`
   - `plan_runs`
   - `dispatch_runs`
   - `publish_state`
2. make replay queue a run and wake dispatch immediately
3. stop using the poll interval to gate execution of already-queued runs
4. teach the runner to execute claimed runs instead of scanning the full queue
5. add a single execution worker with a dispatcher wake signal
6. add a configurable concurrency limit
7. only then consider multi-worker execution if the product still needs it

That order gives fast user-visible wins early without a huge rewrite.

# What assumptions should be explicit?

This plan assumes:

- daemon authority remains in place
- the runtime remains single-process for now
- immediate retry execution is desirable
- commit discovery and run execution should be decoupled
- bounded concurrency is enough; `cid` does not need unbounded parallelism
- the UI and web API continue reading daemon-published snapshots

If `cid` later grows remote workers or multiple executors, this split still helps.
It creates the right boundaries before distribution pressure arrives.

# What are the main risks and tradeoffs?

Main risks:

- introducing too many internal abstractions before the runtime shape settles
- making state transitions implicit again in helper functions with vague names
- allowing dispatch and execution to race on run ownership
- persisting too often in naive ways and adding I/O overhead
- over-prioritizing concurrency before fixing wakeup latency

Tradeoffs:

- the monolithic cycle is simpler, but sluggish
- event boundaries add plumbing, but they let cheap work stay cheap
- an explicit dispatcher is more code, but it creates the right place for future scheduling policy

The main thing to avoid is solving this with "just poll more often forever."
Lower polling helps commit discovery latency, but it is not the right long-term answer for queued-run responsiveness.

# How should this work be tracked?

- [ ] Add an architecture note describing the internal event boundaries and runtime stages
- [x] Split the current daemon cycle into explicit phase helpers
- [x] Make replay queue work and wake dispatch immediately
- [x] Decouple queued-run execution from repository polling cadence
- [ ] Introduce a dispatcher that claims queued runs
- [ ] Change the runner to execute claimed runs instead of scanning all queued runs
- [ ] Add a single-worker execution path triggered by dispatcher wakeups
- [ ] Publish snapshots after planning, dispatch, and execution transitions
- [ ] Add tests proving retries start without waiting for the next poll interval
- [ ] Add tests proving commit discovery continues while execution is busy
- [ ] Add tests for dispatcher claim behavior and no-double-start guarantees
- [ ] Add concurrency limit configuration and tests if that policy is introduced in scope
- [ ] Run `./scripts/check-code.sh`

# How should this be verified?

Verification should focus on responsiveness and correctness.

Add or update tests for:

- replaying a run starts execution without waiting for `poll_interval`
- a newly discovered commit gets queued even while another run is executing
- only one dispatcher claim happens per queued run
- a finished run frees execution capacity and wakes the dispatcher
- snapshots reflect queued, running, and finished transitions in the expected order

Repository-wide verification should include:

- targeted daemon runtime tests
- targeted web tests for retry behavior if the HTTP API changes
- `cargo test`
- `./scripts/check-code.sh`
