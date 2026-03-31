# What is this plan for?

This plan defines the remaining cleanup work for `cid`'s devcontainer-backed runner after the broader execution-model migration landed.

The goal is to tighten the runner behavior, document the enforced repository contract, and add one higher-confidence integration-style verification path without reopening the larger migration plan.

# What problem is this plan solving?

The core devcontainer execution model is already in place, but the remaining gaps are now concentrated in one narrower area:

- the runner still executes CI through `devcontainer up ... --remove-existing-container` before `devcontainer exec`
- the repository execution contract is enforced in code but not documented clearly outside the old migration plan
- there is no sandbox-style integration fixture proving the full contract in one place

That means the product direction is correct, but the runner still does unnecessary per-run work and the documentation story is lagging behind the implementation.

# What is the current status?

Current code already does the important structural work:

- [`crates/daemon/src/config.rs`](/data/projects/cid/crates/daemon/src/config.rs) requires `.devcontainer/devcontainer.json` and `scripts/ci.sh`
- [`crates/daemon/src/daemon.rs`](/data/projects/cid/crates/daemon/src/daemon.rs) fails startup when `devcontainer --version` is unavailable
- [`crates/daemon/src/runner.rs`](/data/projects/cid/crates/daemon/src/runner.rs) computes a devcontainer fingerprint, caches successful builds, builds through `devcontainer build`, and executes CI through the Dev Container CLI

What still looks wrong is the execution command path:

- [`DockerRunner::build_command`](/data/projects/cid/crates/daemon/src/runner.rs) still shells out to `devcontainer up --remove-existing-container && devcontainer exec`
- [`DockerRunner::build_ci_exec_command`](/data/projects/cid/crates/daemon/src/runner.rs) already exists, but it is not the main execution path

That means cached builds help, but repeated runs still churn containers more than necessary.

# What should change?

The follow-up should do three things:

1. document the repository execution contract in prose documentation
2. make the main runner execution path use the cleaner `devcontainer exec` flow directly after build caching
3. add one higher-level sandbox-style fixture or integration test that exercises the contract end to end

This should stay deliberately small.
It is cleanup and hardening, not another architecture rewrite.

# What should the runner do instead of recreating containers every run?

The runner should:

1. compute the devcontainer fingerprint
2. build the devcontainer only when the fingerprint changed
3. execute `scripts/ci.sh` through the existing `devcontainer exec` command path

It should stop forcing `devcontainer up --remove-existing-container` inside the steady-state CI execution path.

That keeps the behavior aligned with the build cache:

- rebuild only when inputs changed
- avoid unnecessary container churn when they did not

# How should the runner avoid stale devcontainers without deleting them every time?

The runner should separate image freshness from container lifecycle freshness.

Image freshness should come from the existing devcontainer fingerprint and build metadata:

- rebuild when the fingerprint changed
- reuse the existing built image when it did not

Container freshness should be handled as a recovery path, not as the default path.

Recommended behavior:

1. ensure the devcontainer image is built for the current fingerprint
2. try the direct `devcontainer exec` path first
3. if `devcontainer exec` fails because the runtime container is missing, stopped, or otherwise invalid, run `devcontainer up`
4. retry `devcontainer exec`

That gives `cid` a simple rule:

- fingerprint changes invalidate images
- runtime exec failures recreate containers

This avoids stale devcontainers without paying the full `--remove-existing-container` cost on every run.

# Where should the repository execution contract be documented?

It should be documented in prose outside the plan system.

Good candidates are:

- [`docs/CONFIG.md`](/data/projects/cid/docs/CONFIG.md), for how repository registration relates to repository-local config
- [`docs/ARCHITECTURE.md`](/data/projects/cid/docs/ARCHITECTURE.md), for the high-level execution contract
- a dedicated repository-contract document if the existing docs start feeling overloaded

The important thing is to make the product contract obvious:

- `.devcontainer/devcontainer.json` is required
- `scripts/ci.sh` is the supported CI entrypoint
- the Dev Container CLI is the execution frontend

# What should not be dragged into this follow-up?

This follow-up should not reopen:

- run-model redesign for separate persisted build/exec phases
- artifact-retention redesign
- generic pipeline abstractions
- multi-worker execution policy

Those are separate questions.
This plan should stay focused on making the current runner implementation cleaner and better defended.

# In what order should the work happen?

Recommended order:

1. document the repository execution contract in prose docs
2. switch the main runner execution path to the direct `devcontainer exec` helper
3. update runner tests so they assert the cleaned-up command path
4. add a sandbox-style integration fixture or test
5. run repository-wide verification

# How should this work be tracked?

- [ ] Document the repository execution contract in prose documentation
- [ ] Switch the main runner execution path off forced `devcontainer up --remove-existing-container`
- [ ] Reuse the direct `devcontainer exec` helper in the steady-state CI execution path
- [ ] Fall back to `devcontainer up` only when direct execution fails because the runtime container is unavailable or stale
- [ ] Update runner tests to assert the cleaned-up execution command path
- [ ] Add a sandbox-style integration fixture or test for `.devcontainer` plus `scripts/ci.sh`
- [ ] Run `./scripts/check-code.sh`

# How should this be verified?

Verification should include:

- runner tests proving cached builds do not require the old forced-container-recreation path
- runner tests proving the CI command uses the direct Dev Container CLI execution path
- at least one higher-level fixture or integration test covering a repository with `.devcontainer` and `scripts/ci.sh`
- repository-wide verification through `./scripts/check-code.sh`
