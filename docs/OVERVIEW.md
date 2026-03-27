# What is `cid`?

`cid` is a local-first continuous integration daemon.
It watches local Git repositories, detects new commits, runs configured build pipelines in Docker images, and stores the results locally for inspection through a web UI.

This document is intentionally high level.
It describes the current direction of the project rather than a frozen implementation contract.

# Why does `cid` exist?

Hosted CI is useful, but it is also slow for the tightest feedback loop.
Many failures are boring local mistakes:

- a formatter violation
- a missing dependency
- a broken test
- a Docker image mismatch

Waiting for a push, remote runner scheduling, and a hosted dashboard to tell you something you could have learned immediately is wasteful.

`cid` exists to move that loop back onto the developer machine while keeping the isolation and visibility that make CI valuable.

# Who is `cid` for?

`cid` is aimed at:

- developers who want automatic local validation for every commit
- small teams that want CI-style visibility without building a whole runner fleet
- multi-repo setups where local branches should be validated before push
- people who want useful reports and trends without depending on a hosted CI provider

# What does `cid` do?

At a high level, `cid` has four responsibilities:

- watch one or more local Git repositories
- decide which new commits need builds
- run those builds in reproducible container environments
- retain logs, reports, and aggregate statistics for later inspection

The daemon should stay running in the background.
When a watched branch advances, it should enqueue a build automatically instead of waiting for a manual command.

# Why use Docker-based execution?

The point is not "containers because containers."
The point is reproducibility.

If a repository declares that a build should run in a specific image, `cid` can execute the same steps with fewer machine-specific surprises.
That makes local validation closer to real CI while still being local-first.

# What should the web UI answer quickly?

The UI should make these questions cheap to answer:

- what is building right now?
- did my last commit pass?
- what failed?
- which repositories are unhealthy?
- are builds getting slower over time?
- which steps fail most often?

That means the UI is not just a log dump.
It should be useful as an operations surface for local development workflows.

# What data should `cid` keep?

`cid` should persist enough local state to make runs inspectable and trends useful:

- watched repositories and branch rules
- detected commits
- queued, running, passed, and failed runs
- per-step logs and outcomes
- durations and timestamps
- aggregate success-rate and timing statistics

The data should stay local and inspectable.

# What should the first version optimize for?

The first version should optimize for:

- simple setup
- reliable commit detection
- boring, reproducible Docker execution
- readable failure reporting
- a straightforward local dashboard

It should not try to become a giant hosted CI clone on day one.

# What is explicitly out of scope for the first version?

At least initially, `cid` should avoid:

- distributed runner orchestration
- enterprise auth systems
- complex pipeline DSL features
- remote artifact retention infrastructure
- every possible VCS host integration

If the local-first developer experience is weak, none of the extra platform stuff matters.
