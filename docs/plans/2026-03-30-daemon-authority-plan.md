# What is this plan for?

This plan defines how `cid` should move to a daemon-owned runtime model where the daemon is the only mutable authority for repositories, runs, and commands, and the web layer becomes a thin stateless adapter.

# What problem is this plan solving?

Today the system has two competing authorities:

- the daemon keeps mutable in-memory state and periodically persists it
- the web server reads and writes the persisted state store directly

That split is exactly how a replayed run could appear briefly and then disappear again:

1. the web layer appends a replayed run to the store
2. the daemon continues from its stale in-memory snapshot
3. the daemon saves its stale snapshot back to disk
4. the replayed run is lost

The recent merge-based fix makes that race less likely to hurt users, but it is still a patch over the wrong ownership model.

If `cid` wants one source of truth, it needs one owner for mutable state.

# What architecture should replace the current model?

`cid` should adopt a daemon-authority architecture:

- the daemon owns all mutable runtime state
- the daemon is the only component allowed to write persisted state
- the daemon is the only component allowed to mutate runs, queue work, or change repository status
- the web layer handles HTTP only and delegates all reads and writes to daemon-owned interfaces

That gives `cid` one boring rule:

> if state changes, the daemon did it

The web server should not load `CidStateStore` directly for API responses and should never write to it.

# What should count as the single source of truth?

The authoritative runtime state should be the daemon's in-memory model.

The persisted store should become:

- a durability layer
- a restart recovery source
- a snapshot written by the daemon

It should not also act as a peer mutation surface for the web thread.

This is the simplest model that makes race conditions structurally impossible inside one process:

- one owner of mutable state
- one writer to persistence
- many read-only consumers

# How should the web layer become dumb and stateless?

The web layer should become a transport adapter over daemon-owned query and command APIs.

Recommended responsibilities:

- parse HTTP requests
- map route params and JSON payloads into daemon queries or commands
- return serialized daemon responses

The web layer should not:

- open the state store
- assemble its own view of current runs from disk
- perform replay logic
- generate run IDs
- write state directly
- keep its own cache of mutable domain state

If the daemon is temporarily unavailable, the web layer should fail the request instead of trying to "help" by mutating storage itself.

# What daemon interface should the web layer call?

Introduce a narrow runtime boundary that separates presentation from authority.

Recommended shape:

- a read-only query interface for snapshots and detail lookups
- a command interface for state changes like replaying a run

For example:

- `query(Query::ListRepositories) -> Response`
- `query(Query::GetRepository { name }) -> Response`
- `query(Query::GetRun { id }) -> Response`
- `command(Command::ReplayRun { run_id }) -> Response`

That boundary should live in daemon-facing code, not in the web crate.
The web crate should depend only on the interface, not on persistence details.

# How should the daemon serve concurrent queries without giving up authority?

The daemon should own mutation on one execution path and publish immutable snapshots for readers.

Recommended model:

1. the daemon thread owns mutable `DaemonState`
2. after every meaningful state transition, it publishes an immutable snapshot
3. query handlers read that snapshot
4. command handlers send requests to the daemon and wait for the daemon's response

That can be implemented with simple local-process primitives:

- `Arc<RwLock<DaemonSnapshot>>` for published read models
- a command channel into the daemon event loop for mutations
- per-command response channels for request/response behavior

This keeps read paths cheap without letting the web layer mutate anything.

# What should happen to replay requests under this model?

Replay should become a daemon command, not a web persistence operation.

Recommended flow:

1. `POST /api/runs/:id/replay` reaches the web layer
2. the web layer sends `Command::ReplayRun { run_id }` to the daemon
3. the daemon validates the source run and repository
4. the daemon allocates the new run ID
5. the daemon appends the run to in-memory state
6. the daemon persists the updated state
7. the daemon republishes the snapshot
8. the web layer returns the daemon's response

Under that flow, the replayed run cannot disappear because no second authority exists to overwrite it.

# How should persistence fit after this change?

Persistence should become daemon-owned write-through state storage.

Recommended rules:

- only the daemon writes through `CidStateStore::save`
- startup still restores the daemon state from `CidStateStore::load`
- command handlers persist before acknowledging success
- periodic daemon work persists after state changes
- the web layer never touches `CidStateStore`

That means the store stops being a shared coordination mechanism and returns to being what it should be: persistence.

# Should query responses come from in-memory snapshots or the store?

They should come from daemon-published snapshots.

Reasons:

- they represent the daemon's authoritative current view
- they avoid disk reads on every request
- they avoid mixed answers where one request sees memory state and another sees stale persisted state
- they make "live" and "historical" data the same model

If a snapshot is not rich enough for a query, enrich the snapshot or add a daemon-owned query helper.
Do not teach the web layer to go behind the daemon's back.

# What runtime shape should the server process adopt?

The server process should still run daemon work and HTTP serving in one process, but with a clear ownership split:

- one daemon runtime instance
- one web adapter using daemon handles
- zero direct store access from the web adapter

Conceptually:

```text
HTTP request
    |
    v
stateless web adapter
    |
    +--> query snapshot
    |
    +--> send command to daemon
              |
              v
        daemon event loop
              |
              +--> mutate in-memory state
              +--> persist state
              +--> publish snapshot
```

# What code structure changes are needed?

Recommended refactor:

1. define daemon-facing query and command types
2. introduce a `DaemonHandle` or similarly named façade for non-daemon callers
3. move replay logic out of `crates/web` and into daemon-owned command handling
4. move API read helpers off direct store access and onto daemon snapshots
5. make `crates/web` depend on the façade instead of `CidStateStore`
6. keep persistence internals private to daemon code wherever possible

This should leave `crates/web` mostly about:

- routing
- request decoding
- response encoding

# What implementation approach is recommended?

Implement this in phases so behavior stays testable.

Phase 1: define the authority boundary

- add daemon query and command enums or request types
- add a daemon-owned handle that exposes query and command entry points
- keep existing daemon loop behavior, but route replay through the daemon

Phase 2: publish a read snapshot

- define a read model tailored for the API surface
- publish snapshots from the daemon after state changes
- serve web reads from the snapshot instead of the store

Phase 3: remove web-store coupling

- remove `CidStateStore` construction from `crates/web`
- delete direct persistence reads and writes from the web crate
- tighten module visibility so the store is not casually reused from presentation code

Phase 4: harden the runtime contract

- add concurrency tests for command handling during daemon cycles
- document the new ownership model in architecture docs
- add guardrails that prevent new direct store access from creeping back into web code

# What assumptions should be explicit?

This plan assumes:

- `cid` remains a single-process local daemon plus embedded web server for now
- immediate consistency between commands and subsequent reads is desirable
- one mutable daemon authority is more valuable than allowing ad hoc writes from helper threads
- query traffic is light enough that snapshot publication is simpler than a fully async shared-state architecture
- the API surface can tolerate daemon-mediated access instead of direct SQLite reads

If `cid` later becomes multi-process or distributed, the same ownership rule should still hold, but the transport will need to change.

# What are the main risks and tradeoffs?

Main risks:

- overdesigning the command/query boundary too early
- publishing a snapshot that is too thin and forces awkward follow-up queries
- holding coarse locks for too long and making the web UI feel sluggish
- partially migrating the web layer and leaving a confusing hybrid architecture behind

Tradeoffs:

- direct store reads are simple, but they are not authoritative
- daemon-owned snapshots add a little plumbing, but they remove an entire class of lost-update bugs
- command serialization may slightly limit parallel mutation throughput, but `cid` does not need high write concurrency to be good

This is a case where boring serialization beats clever shared mutability.

# In what order should implementation happen?

Recommended order:

1. add a short architecture note to [docs/ARCHITECTURE.md](/data/projects/cid/docs/ARCHITECTURE.md) describing daemon authority and stateless presentation
2. define daemon query and command request/response types in daemon-owned code
3. add a daemon façade that the server can share with the web layer
4. move replay behavior into daemon command handling
5. publish daemon-owned read snapshots
6. switch web GET endpoints to snapshot-backed queries
7. remove direct `CidStateStore` usage from the web crate
8. tighten visibility so presentation code cannot reach persistence by accident
9. add regression tests for command execution during daemon cycles
10. run repository-wide verification

# How should this work be tracked?

- [x] Document daemon authority and stateless presentation in [docs/ARCHITECTURE.md](/data/projects/cid/docs/ARCHITECTURE.md)
- [x] Define daemon-owned query request and response types
- [x] Define daemon-owned command request and response types
- [x] Introduce a daemon façade or handle for external callers
- [x] Route replay requests through daemon command handling instead of direct store writes
- [x] Publish an immutable daemon snapshot for read queries
- [x] Serve repository, branch, run, and summary API reads from the daemon snapshot
- [x] Remove direct `CidStateStore` reads and writes from [crates/web/src/lib.rs](/data/projects/cid/crates/web/src/lib.rs)
- [ ] Tighten module visibility so persistence is not a presentation dependency
- [ ] Add daemon-level regression tests for command handling during active run cycles
- [x] Add web tests that verify replay requests round-trip through the daemon interface
- [ ] Run `./scripts/check-code.sh`

# How should this be verified?

Verification should prove both architecture and behavior.

Add or update tests for:

- replay requests while the daemon is idle
- replay requests while the daemon is mid-cycle
- repeated GET requests after replay returning stable results
- restart recovery loading the replayed run correctly from persisted state
- web handlers functioning with a daemon façade and no direct store access

Repository-wide verification should include:

- `cargo test`
- targeted tests for daemon command/query handling
- targeted tests for web adapter behavior
- `./scripts/check-code.sh`
