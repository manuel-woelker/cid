# AGENTS.md

This file provides guidance to human developers and AI agents working in this repository.

## Project Overview

`cid` is a local-first continuous integration daemon.
It watches local Git repositories, schedules builds for new commits, runs those builds in Docker-based environments, and exposes run reports and statistics through a web UI.

All developer-facing documentation in this repository should be written in English.

## Tech Stack

- Implementation language: Rust
- Keep shared cross-crate primitives in `crates/base`
- Prefer readable, explicit data structures over clever abstractions
- Optimize for maintainability first, then performance where it materially matters

## Documentation Strategy

Consult [docs/PLANS.md](/data/projects/cid/docs/PLANS.md) when creating or updating plan documents in `docs/plans`.

Prefer question-driven headings in prose documentation.
That forces the document to answer something concrete instead of drifting into vague notes.

Use standard Rust documentation comments for public types and functions.
Use inline rationale comments only when the "why" is non-obvious.

## Testing Strategy

Consult [docs/TESTING.md](/data/projects/cid/docs/TESTING.md) when adding or changing tests.

Default expectations:

- tests should be colocated with the code they verify
- prefer black-box behavior tests over mocking
- add regression coverage for bug fixes
- run `nao check` before considering a unit of work done

## Commit Messages

Use Conventional Commits for commit subjects.

Examples:

- `feat(cli): add status subcommand`
- `chore(repo): add CI workflow`

Always run `git add` and `git commit` as separate commands.
Never push code unless the user explicitly asks for it.

## File Organization

Prefer small source files with descriptive names.

- keep `lib.rs` focused on module declarations and re-exports
- avoid catch-all files like `types.rs` when a clearer name exists
- split modules when a file starts carrying multiple unrelated responsibilities
