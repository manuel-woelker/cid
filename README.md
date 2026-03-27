# cid

`cid` is a local-first continuous integration daemon.

It watches one or more local Git repositories, detects new commits, builds and tests them in Docker-based job environments, and exposes a web UI with run history, reports, and repository statistics.

The goal is simple: bring the fast feedback loop of CI to your own machine, without needing to push every branch to a remote service first.

## Why cid?

Most CI systems are remote-first:

- Push commit
- Wait for a webhook
- Wait for a runner
- Open a web page
- Learn that you forgot a dependency or broke a test 10 minutes ago

`cid` flips that model around.

It runs next to your repos, notices new commits immediately, executes your pipeline locally in isolated Docker images, and gives you a clean dashboard for build status, failures, trends, and repo health.

That makes it useful for:

- fast pre-push validation
- branch-heavy local development
- multi-repo setups
- offline or low-connectivity work
- small teams that want CI visibility without CI sprawl
- personal projects where GitHub Actions is overkill

## Core idea

`cid` combines four pieces:

1. A daemon that watches local repositories for new commits
2. A scheduler that decides what should be built and when
3. A runner that executes jobs inside Docker images
4. A web UI that shows build reports, failures, timings, and trends

In practice, that means:

- you register one or more local repos with `cid`
- `cid` watches their Git state
- when a new commit appears on a tracked branch, it creates a build
- the build runs inside a configured container image
- logs, artifacts, exit status, and timing data are stored locally
- the web UI shows current status and historical statistics

## Features

- Local-first CI for Git repositories on your machine
- Automatic detection of new commits
- Docker-based isolated build environments
- Per-repository pipeline configuration
- Build logs and structured reports
- Commit-level history and status tracking
- Web dashboard with recent runs and failure summaries
- Statistics such as pass rate, duration trends, and flaky job detection
- Designed to work without a remote Git host

## Example workflow

You commit locally:

```bash
git commit -am "refactor parser"
```

`cid` notices the new commit in a watched repository and runs something like:

```yaml
image: rust:1.88
steps:
  - cargo fmt --check
  - cargo clippy --all-targets --all-features -- -D warnings
  - cargo test --all
```

Then the web UI shows:

- commit SHA and branch
- running, passed, or failed state
- per-step logs
- total duration
- recent success rate for the repo

## Configuration sketch

One possible repo config could look like this:

```yaml
repo:
  name: cid
  path: /home/user/src/cid
  branches:
    - main
    - feature/*

pipeline:
  image: rust:1.88
  workdir: /workspace
  steps:
    - name: fmt
      run: cargo fmt --check
    - name: lint
      run: cargo clippy --all-targets --all-features -- -D warnings
    - name: test
      run: cargo test --all

artifacts:
  paths:
    - target/nextest
    - coverage/
```

The exact config format is still open, but the shape should stay boring and predictable.

## Web UI

The UI should answer a few questions quickly:

- What is building right now?
- Did my last commit pass?
- What failed?
- Is this repo getting slower?
- Which jobs are flaky?

Useful views:

- repository overview
- recent runs
- run detail with step logs
- branch health
- commit history
- duration and success-rate charts
- top failing steps across time

## Design goals

- Local-first, not cloud-dependent
- Fast feedback over distributed complexity
- Reproducible builds through containerized execution
- Simple setup for solo developers and small teams
- Useful by default, extensible later

## Non-goals

At least initially, `cid` should avoid becoming a full clone of hosted CI platforms.

That means deprioritizing things like:

- complex distributed runner orchestration
- enterprise auth and permissions systems
- deeply dynamic pipeline DSLs
- remote-secret management platforms
- massive artifact retention infrastructure

If the local-first experience is not excellent, the rest is noise.

## Rough architecture

```text
local git repos
      |
      v
 repo watcher
      |
      v
   scheduler
      |
      v
 docker job runner
      |
      +--> logs
      +--> artifacts
      +--> metadata store
                |
                v
             web API
                |
                v
              web UI
```

## Storage

`cid` should keep its state local and inspectable.

Likely data to persist:

- repositories and watched branches
- detected commits
- run metadata
- step results
- timing history
- artifact references
- aggregated stats for the dashboard

A lightweight embedded database is probably enough for a long time.

## Future ideas

- manual rebuilds
- file-change-aware step skipping
- local notifications
- Git hook integration
- side-by-side run comparison
- test failure clustering
- remote agent mode for a second machine

## Status

This project is currently at the definition stage.

The README is intended to describe the product clearly before implementation starts:

- what it does
- why it exists
- what the first version should optimize for

## Name

`cid` stands for continuous integration daemon.

Short, blunt, and a little Unix-y, which feels right.
