# What is this plan for?

This plan defines how `cid` should execute repository builds in an opinionated container model.
Instead of treating the repository pipeline as an arbitrary list of commands in an arbitrary image, `cid` should expect a Dev Container configuration, use the Dev Container CLI to build and run that environment, and execute the repository's `scripts/ci.sh` entrypoint inside it.

# What problem is this plan solving?

Today the runner in [`crates/daemon/src/runner.rs`](/data/projects/cid/crates/daemon/src/runner.rs) executes each pipeline step by starting:

- `docker run`
- a configured image from repository config
- `sh -c <step command>`

That model is simple, but it leaves too much build-environment responsibility scattered across repository config:

- the runtime image is configured in `.cid/cid.yaml`
- the CI command graph is duplicated across pipeline steps
- editor/dev tooling and CI tooling can drift apart
- repositories must express environment setup and command selection through `cid` instead of through their own devcontainer and scripts

If `cid` is opinionated, it should stop pretending to be a general-purpose pipeline runner.
It should standardize the execution contract around:

- a repository-local `.devcontainer`
- a repository-local `scripts/ci.sh`

# What is the current status?

The current implementation still models execution as a per-step pipeline:

- [`Repository::pipeline`](/data/projects/cid/crates/daemon/src/repository.rs) stores an image plus ordered steps
- [`DockerRunner::build_command`](/data/projects/cid/crates/daemon/src/runner.rs) mounts the repository into `/workspace` and executes one shell command inside the configured image
- repository config parsing in [`crates/daemon/src/config.rs`](/data/projects/cid/crates/daemon/src/config.rs) still expects `pipeline.image` and `pipeline.steps`

The sandbox repository under [`sandboxes/cid-rust-sandbox`](/data/projects/cid/sandboxes/cid-rust-sandbox) now reflects the intended future shape:

- a `.devcontainer` directory
- a `.nao/nao.kdl` task graph
- a `scripts/ci.sh` script that runs `nao ci`

What is missing is making `cid` treat that shape as the product contract instead of just repository convention.

# What execution model should `cid` adopt?

`cid` should use one boring execution path:

1. verify that the repository contains a supported devcontainer definition
2. build the devcontainer environment for that repository through the Dev Container CLI
3. run `scripts/ci.sh` inside the built container
4. capture stdout, stderr, exit code, timing, and artifacts

This model should be opinionated on purpose.
`cid` is not trying to be GitHub Actions, Tekton, or a custom YAML DSL.
It is trying to run "the repo's own development container plus the repo's own CI entrypoint" in a predictable way.

# What repository contract should be enforced?

The repository contract should be:

- `.devcontainer/devcontainer.json` must exist
- the devcontainer configuration must point at a buildable Dockerfile or image
- `scripts/ci.sh` must exist and be executable
- the host running `cid` must have a working `devcontainer` CLI

For the first version, `cid` should support only the simplest devcontainer shape that the project actually wants to rely on.
That likely means:

- one `devcontainer.json`
- one Dockerfile-based build path
- one workspace mount convention
- one CI entrypoint path: `scripts/ci.sh`

It should not try to support every corner of the Dev Container spec on day one.

# What should happen to the current pipeline model?

The current `pipeline.image` and `pipeline.steps` model should be retired or reduced to compatibility shims.

Recommended direction:

- remove `image` and `steps` from the long-term repository execution contract
- replace them with a smaller execution descriptor, such as `build_strategy = "devcontainer"`
- keep artifact declaration support if it remains useful for retention
- migrate repository config validation so the main checks are about `.devcontainer` and `scripts/ci.sh`, not YAML-defined step lists

If a temporary migration period is needed, `cid` can keep parsing the old pipeline shape while preferring the new contract.
That compatibility layer should be explicit and temporary, not open-ended.

# How should the runner build and execute the devcontainer?

The runner should split execution into two stages:

1. devcontainer resolution and build
2. run execution

Recommended runner behavior:

- invoke the Dev Container CLI instead of reimplementing devcontainer semantics inside `cid`
- derive a stable cache identity from repository identity and a hash of the devcontainer inputs
- build the devcontainer before starting the run itself
- reuse the built environment when the devcontainer inputs have not changed
- execute `scripts/ci.sh` through the devcontainer runtime path rather than a raw `docker run` assembled by `cid`

The runner should not rebuild the devcontainer for every run if the devcontainer inputs are unchanged.
That would make local CI feel sluggish for no good reason.

# How should `cid` make sure the correct devcontainer image is built and used after config changes?

`cid` should treat the devcontainer definition as versioned build input and should never assume the last built image is still correct.

Recommended behavior:

- compute a deterministic fingerprint from the devcontainer inputs before each run
- compare that fingerprint with the last successfully built fingerprint stored for the repository
- rebuild the devcontainer when the fingerprint has changed
- reuse the previously built image only when the fingerprint matches exactly
- record the image tag and fingerprint used by each run so later debugging can answer "what environment did this commit actually use?"

For the first version, the fingerprint should include at least:

- `.devcontainer/devcontainer.json`
- the referenced Dockerfile
- any repository-local files referenced by the devcontainer configuration that materially affect the build

The implementation should bias toward rebuilding when input tracking is uncertain.
Using a slightly stale cache is worse than rebuilding once too often.

# How should `cid` verify that Dev Container execution is available?

`cid` should verify Dev Container CLI availability during startup instead of discovering the problem only when the first run fails.

Recommended startup behavior:

- run `devcontainer --version`
- fail startup if that command cannot be executed successfully
- surface a blunt error that tells the user `cid` requires the Dev Container CLI on the host

That check should happen alongside other environment validation.
If `cid` is opinionated about execution, it should say so immediately.

# What devcontainer scope should the first version support?

The first version should support only the smallest useful subset:

- `.devcontainer/devcontainer.json`
- Dockerfile-backed builds
- Dev Container CLI-driven build and execution
- optional `postCreateCommand` support only if it is required for the repository contract

The first version should explicitly not optimize for:

- Docker Compose devcontainers
- remote container features
- lifecycle parity with every Dev Container CLI behavior
- editor-specific customization fields beyond what is needed to build the image

If the runner can deterministically build the container image and run `scripts/ci.sh`, it has solved the core product need.

# How should build caching work?

`cid` should cache devcontainer build outputs by repository and devcontainer definition fingerprint.

The cache key should include at least:

- the contents of `.devcontainer/devcontainer.json`
- the referenced Dockerfile contents
- any referenced files copied into the build context that materially affect the image

The runner should store enough metadata to answer:

- what image tag was last built for this repository
- what fingerprint produced that tag
- when the image was last built

Without a cache, the product will feel wasteful.
With a sloppy cache, the product will feel haunted.
This part needs to be boring and deterministic.

# How should logs and run phases be represented?

The current run model assumes step-level execution, but the new path has at least two distinct phases:

- devcontainer image build
- CI script execution

Recommended direction:

- model the build phase explicitly in the run event stream
- preserve a single top-level run result for the UI
- decide whether the image-build phase should appear as its own synthetic step in run details

A synthetic-step model is probably the least disruptive option:

- `build devcontainer`
- `run ci script`

That keeps the UI and persistence model coherent without pretending the devcontainer build is invisible.

# What should validation do before a run starts?

Before queueing or starting execution, `cid` should validate:

- `.devcontainer/devcontainer.json` exists
- the referenced Dockerfile exists if the repository uses one
- `scripts/ci.sh` exists
- `scripts/ci.sh` is executable, or can be executed via `sh`

The error messages should be blunt and actionable.
For example:

- "repository is missing .devcontainer/devcontainer.json"
- "repository is missing scripts/ci.sh"
- "repository devcontainer references Dockerfile X but that file does not exist"

# What assumptions should be made explicit?

This plan assumes:

- `cid` will remain Docker-based for execution
- `cid` will invoke the Dev Container CLI as the execution frontend
- repositories are willing to treat the devcontainer as the canonical build environment
- `scripts/ci.sh` is the one supported CI entrypoint path
- the sandbox repository is the first concrete example of the desired contract, not a one-off
- supporting the entire Dev Container spec is out of scope for the first implementation

If those assumptions change, the runner design should change with them instead of accreting exceptions.

# What are the main risks?

The main risks are:

- implementing only half of the Dev Container CLI contract and surprising users with inconsistent behavior
- assuming the Dev Container CLI is present and healthy without validating it at startup
- rebuilding images too often and making repeated local runs annoyingly slow
- under-hashing the build inputs and reusing stale images
- overfitting the runner to one sandbox repository in ways that do not generalize
- trying to preserve too much of the old pipeline model and ending up with two competing execution systems

# In what order should the implementation happen?

Recommended order:

1. define the repository execution contract in docs and code comments
2. introduce a new repository execution model that represents devcontainer-backed CI instead of image-plus-steps
3. add config validation for `.devcontainer` and `scripts/ci.sh`
4. add startup validation that runs `devcontainer --version`
5. teach the runner to compute a devcontainer build fingerprint and stable cache identity
6. teach the runner to build the repository devcontainer through the Dev Container CLI
7. teach the runner to execute `scripts/ci.sh` through the Dev Container CLI
8. represent the devcontainer build and CI execution phases in logs and persisted run metadata
9. update the UI and API if needed so the new execution phases are understandable
10. remove or deprecate the old pipeline-step execution path

# How should this work be tracked?

- [ ] Document the new repository execution contract in prose documentation
- [ ] Replace or adapt the repository execution data model in [`crates/daemon/src/repository.rs`](/data/projects/cid/crates/daemon/src/repository.rs)
- [ ] Update config loading and validation in [`crates/daemon/src/config.rs`](/data/projects/cid/crates/daemon/src/config.rs) to require `.devcontainer` and `scripts/ci.sh`
- [ ] Add startup validation that runs `devcontainer --version` and fails clearly if the CLI is unavailable
- [ ] Add a runner helper that resolves devcontainer inputs and computes a stable build fingerprint
- [ ] Add a runner helper that derives a stable cache identity from repository identity plus fingerprint
- [ ] Add runner logic that builds the devcontainer environment through the Dev Container CLI before CI execution
- [ ] Add runner logic that executes `scripts/ci.sh` through the Dev Container CLI
- [ ] Decide how artifact retention should work once step-level pipeline commands are removed
- [ ] Update persistence in [`crates/daemon/src/persistence.rs`](/data/projects/cid/crates/daemon/src/persistence.rs) if run-phase metadata changes
- [ ] Add or update colocated tests for config validation failures and success cases
- [ ] Add or update runner tests for `devcontainer --version`, devcontainer build command generation, and CI execution command generation
- [ ] Add an end-to-end sandbox-style fixture that proves `cid` can build a devcontainer and run `scripts/ci.sh`
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- config-loading tests that reject repositories missing `.devcontainer/devcontainer.json`
- config-loading tests that reject repositories missing `scripts/ci.sh`
- startup validation tests that assert `cid` checks `devcontainer --version`
- runner tests that assert the expected Dev Container CLI commands are generated
- cache-key tests that prove devcontainer input changes invalidate the cached image
- persistence tests if the run model or stored metadata changes
- a sandbox-style integration test that exercises the full contract on a repository with `.devcontainer`, `.nao`, and `scripts/ci.sh`
- repository-wide verification through `./scripts/check-code.sh`

# What improvements or follow-up work are worth considering?

Useful follow-up ideas, but outside the core implementation:

- support for devcontainer image reuse across repositories that intentionally share the same environment
- surfacing the active devcontainer image hash in the UI for debugging
- a "warm build environment" command that prebuilds repository containers before the next commit lands
- a future compatibility mode for legacy repositories that still use the old pipeline-step contract
