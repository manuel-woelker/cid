# What is this plan for?

This plan defines how `cid` should simplify execution handoff by moving to a decoupled, channel-based executor model.

The goal is to make dispatch behavior easier to reason about, make daemon tests less brittle, and keep the daemon as the only authority over run state transitions.

# What problem is this plan solving?

The current execution path works, but the testing seam is awkward:

- the daemon dispatches work and tracks execution capacity
- a worker thread executes runs through the runner
- daemon tests currently need to infer "execution has started and is still in progress" through low-level process behavior

That is the wrong boundary for daemon tests.

The daemon should care about:

- whether a run was claimed
- whether execution capacity is occupied
- whether a completion arrived

It should not need to care how the runner happens to launch host processes.

One explicit goal of this plan is to simplify the daemon tests.
If the new execution handoff still requires daemon tests to understand process-command details, the refactor has missed the point.

# What architecture should replace the current execution handoff?

`cid` should use an explicit execution-request channel plus an explicit completion channel.

Recommended model:

1. the daemon claims a queued run
2. the daemon marks it `Running`
3. the daemon persists and publishes the updated state
4. the daemon sends an `ExecutionRequest` to the executor channel
5. the worker receives the request and executes it
6. the worker sends `ExecutionFinished` back to the daemon
7. the daemon merges the completed run, persists, publishes, and wakes dispatch if more queued work exists

That keeps the handoff explicit and narrow.

# What should the daemon remain responsible for?

The daemon should remain responsible for all authoritative run-state transitions:

- `Queued -> Running`
- `Running -> Passed`
- `Running -> Failed`

It should also remain responsible for:

- capacity tracking
- persistence
- snapshot publication
- deciding when another run may start

The executor should not mutate daemon state directly.
It should only consume requests and produce completions.

# What should the executor be responsible for?

The executor should do only slow side effects:

- run the claimed job through the runner
- return the completed run result

It should not:

- inspect the daemon queue
- decide what to claim next
- mutate authoritative state
- publish snapshots

That keeps the executor boring and narrow.

# What channel types should be introduced?

Recommended types:

- `ExecutionRequest { repository, run }`
- `ExecutionFinished { run }`

The daemon should own the sender for execution requests and the receiver for completion events.
The worker thread should own the receiver for execution requests and the sender for completion events.

If more metadata is needed later, it can be added to those payloads.
The important part is that the handoff is explicit.

# How should this improve testability?

This model makes daemon tests much cleaner:

- they can assert that dispatch produced an `ExecutionRequest`
- they can verify that the run was marked `Running` before the request was sent
- they can simulate completion by sending an `ExecutionFinished` message back
- they no longer need to block host-process execution through `Pal`

That means:

- runner tests keep using `PalMock` to verify command behavior
- daemon tests verify queue, dispatch, capacity, and completion behavior at the daemon boundary

This removes the need for process-shape-dependent daemon test helpers.
The daemon tests should end up shorter, more direct, and focused on queue and completion behavior rather than on mocking host-process timing.

# What should stay out of scope?

Cancellation and interruption are explicitly out of scope for this plan.

This work should not attempt to add:

- cancel signals
- process termination support
- run interruption semantics
- multi-worker execution policy changes

The goal is only to decouple handoff and simplify testing, not to redesign execution control.

# How should the implementation be structured?

Recommended structure:

1. define `ExecutionRequest`
2. replace ad hoc task submission with an explicit request sender
3. keep the existing completion command path, but make it clearly paired with the request channel
4. add a small daemon helper that claims a run and returns or submits an execution request
5. update daemon tests to inspect dispatched requests and inject completions instead of blocking execution through `Pal`
6. remove the brittle daemon-side blocking test helper

This should keep the production behavior nearly unchanged while making the handoff much clearer.

# What implementation order is recommended?

Recommended order:

1. introduce explicit `ExecutionRequest` and completion plumbing
2. refactor daemon dispatch to submit requests through that channel
3. keep the worker thread behavior the same, but make it consume `ExecutionRequest`
4. rewrite daemon tests to use the request/completion boundary
5. remove the old blocking `Pal`-based daemon test helper
6. run repository-wide verification

# How should this work be tracked?

- [ ] Introduce an explicit `ExecutionRequest` type for claimed work
- [ ] Make daemon dispatch submit `ExecutionRequest` values through a dedicated channel
- [ ] Keep executor completion flowing back through an explicit completion path
- [ ] Add daemon helpers that expose dispatch/completion behavior without relying on low-level process details
- [ ] Rewrite daemon runtime tests to assert dispatched requests and inject completions directly
- [ ] Remove the brittle daemon-side blocking execution test helper
- [ ] Run `./scripts/check-code.sh`

# How should this be verified?

Verification should include:

- daemon tests proving replay dispatches a request immediately
- daemon tests proving discovery can continue while execution capacity is occupied
- daemon tests proving capacity is freed only after completion arrives
- runner tests continuing to validate actual process-command behavior separately
- repository-wide verification through `./scripts/check-code.sh`
