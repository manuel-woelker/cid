# What is this plan for?

This plan defines how `cid` should move the devcontainer runner from error-string-based stale-container recovery to explicit container identity using Dev Container CLI `--id-label` support.

The goal is to make container selection deterministic, reduce brittle stderr matching, and align runtime behavior with the existing devcontainer fingerprint model.

# What problem is this plan solving?

The current runner already does the right thing for image freshness:

- it computes a devcontainer fingerprint
- it rebuilds the image when the fingerprint changes
- it reuses the cached image when the fingerprint matches

But runtime container freshness is still handled reactively by parsing `devcontainer exec` failures and falling back to `devcontainer up`.

That works as a patch, but it is brittle because:

- it depends on matching error strings
- Dev Container CLI error text can vary
- the runner is inferring container identity indirectly instead of selecting it explicitly

Now that both `devcontainer up` and `devcontainer exec` support `--id-label`, `cid` can move to a more deterministic model.

# What architecture should replace the current stale-container detection?

The runner should treat the runtime container identity as a function of:

- repository identity
- devcontainer fingerprint

Recommended model:

1. compute the devcontainer fingerprint
2. derive stable id labels from repository identity plus fingerprint
3. run `devcontainer exec` with those id labels
4. if no matching runnable container exists, run `devcontainer up` with the same id labels
5. retry `devcontainer exec`

That makes container lookup explicit instead of relying on inferred workspace-based identity or parsing runtime errors.

# What labels should be used?

Recommended labels:

- `cid.repository=<repository-name>`
- `cid.devcontainer-fingerprint=<fingerprint>`

The exact label names can be adjusted, but they should be:

- stable
- `cid`-owned
- specific enough to avoid accidental collisions

If repository name alone is not considered stable enough, the repository id or a slugged path-derived identity can be included instead.

# Why is this better than a preflight `devcontainer exec echo ...` check?

An explicit id-label model is better than adding a separate preflight command because:

- it avoids an extra process invocation on every run
- the real CI command becomes the runtime availability check
- the runner selects the intended container identity directly
- fingerprint changes naturally select a different container

That keeps the control flow simpler:

- choose identity
- try exec
- create if missing
- retry exec

# How should this interact with the existing build cache?

The build cache should remain the source of truth for image freshness.

The id-label model should only change runtime container selection.

That means:

- fingerprint mismatch still triggers image rebuild
- fingerprint also changes the runtime container identity
- an old container with an old fingerprint label is naturally ignored

This is the clean split:

- build metadata decides whether the image is current
- id labels decide whether the runtime container matches that image identity

# What should be removed or reduced after this change?

After this lands, the current error-string-based stale-container detection should be reduced or removed.

It is reasonable to keep a small fallback for unexpected CLI behavior during migration, but it should stop being the primary mechanism.

The runner should no longer rely mainly on matching strings like:

- `container is not running`
- `container state improper`
- `shell server terminated`

Those should become a safety net, not the main design.

# What implementation approach is recommended?

Recommended order:

1. add a helper that derives stable devcontainer id labels from repository identity plus fingerprint
2. update `devcontainer up` command generation to include those labels
3. update `devcontainer exec` command generation to include those labels
4. refactor execution flow so missing/runnable-container recovery is driven primarily by id-label lookup behavior
5. reduce the old stderr-matching fallback to a secondary safety net or remove it if the new behavior is sufficient
6. update runner tests for the new command shape and runtime-selection behavior
7. run repository-wide verification

# How should this work be tracked?

- [x] Add a helper that derives stable `--id-label` values from repository identity plus devcontainer fingerprint
- [x] Update `devcontainer up` command generation to include the id labels
- [x] Update `devcontainer exec` command generation to include the id labels
- [x] Refactor runner execution flow so id-label-based lookup is the primary container-selection mechanism
- [x] Reduce or remove the old stderr-matching stale-container detection path
- [x] Update runner tests for the label-based command shape and fallback behavior
- [x] Run `./scripts/check-code.sh`

# How should this be verified?

Verification should include:

- runner tests proving `devcontainer exec` includes the expected id labels
- runner tests proving `devcontainer up` includes the expected id labels
- runner tests proving a fingerprint change changes the selected runtime container identity
- runner tests proving execution still recovers by creating the labeled container when needed
- repository-wide verification through `./scripts/check-code.sh`
