# What is this document for?

This document describes the testing expectations for `cid`.
Use it when adding features, fixing bugs, or changing behavior that should remain stable over time.

# Where should tests live?

Tests should usually be colocated with the code they verify.

In practice, that means:

- unit tests live in the same Rust source file behind `#[cfg(test)]`
- crate-level integration tests are fine when the public API is easier to exercise from outside
- documentation-only changes do not need Rust tests unless they imply behavior changes

# What testing style should be preferred?

Prefer small black-box tests that verify observable behavior.

Prefer:

- data-driven tests when several inputs exercise the same rule
- snapshot-style assertions when rendered output matters
- regression tests for every bug fix that changes behavior

Avoid mocking unless the real dependency boundary is expensive or nondeterministic.

# What should be verified for CLI and shared-base changes?

CLI and base-layer changes should usually verify:

- user-visible output
- error formatting
- argument handling
- exit status behavior
- serialization or parsing behavior when formats matter

If a bug fix changed one summary line or one error case, add a focused test for that exact behavior instead of broad, fuzzy coverage.

# What should be verified for daemon and runner changes?

Daemon and runner changes should usually verify:

- commit detection behavior
- build scheduling decisions
- Docker command construction
- persisted run metadata
- failure and retry behavior

Prefer tests that assert on observable state transitions over implementation details.

# What repository-wide checks should be run?

When completing a unit of work, run:

```bash
./scripts/check-code.sh
```

That script should stay boring:

- formatting
- build
- clippy
- tests

# What should happen when a change is not tested?

Call it out explicitly in the final summary.
If a change is intentionally left without automated coverage, explain why the normal testing path was not practical.
