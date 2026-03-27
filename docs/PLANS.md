# What is this document for?

This document defines how planning documents should be created, structured, maintained, and completed in this repository.
Use it whenever you create a new plan or update an existing one in `docs/plans`.

# Where should plan documents live?

Active plans belong in `docs/plans`.
Completed plans belong in `docs/plans/completed`.

Do not keep finished plans in the active plans directory.
Move the file into `docs/plans/completed` once the work is done.

# How should plan files be named?

Prefix each plan filename with an ISO-8601 date in `YYYY-MM-DD` format.
After the date, add a short kebab-case description of the work.

Use this pattern:

```text
docs/plans/YYYY-MM-DD-short-description-plan.md
```

Example:

```text
docs/plans/2026-03-14-equality-branching-plan.md
```

# How should a plan be structured?

Write plan headings as questions, consistent with the repository documentation style.
The plan should explain the problem, the intended implementation approach, the verification strategy, and any important assumptions.

A good plan should usually include:

- the problem being solved
- the current status or observed gap
- the intended implementation approach
- the recommended implementation order
- a checklist or task list
- the verification approach
- assumptions, risks, or open questions when relevant

# How should tasks be tracked inside a plan?

Use a Markdown checklist for concrete implementation and verification work.
The checklist should make it obvious what is done, what is still pending, and how completion will be judged.
Update the checklist as work lands so implemented tasks are marked complete instead of leaving the plan stale.

Include verification tasks in the checklist rather than leaving verification implicit.
For example, include items for adding fixtures, running verification, and running repository-wide checks when those are part of the work.

# What should be documented as assumptions?

Document assumptions whenever the plan depends on facts that may change, constraints that are not yet enforced, or decisions that still need validation.
Examples include Git-watch limitations, Docker execution assumptions, temporary feature flags, schema gaps in stored run metadata, or UI constraints that will be enforced later.

Keep assumptions explicit so later contributors can distinguish between completed work and work that only appears complete under a narrow set of conditions.

# How should verification be planned?

Every plan should describe how the work will be verified.
Prefer explicit verification steps over generic statements like "test it."

When relevant, include tasks for:

- adding or updating colocated tests
- adding or updating test fixtures or sample inputs used by the affected code
- running `./scripts/check-code.sh`

If the work is intentionally not covered by one of these mechanisms, state that clearly.

# When should a plan move to the completed directory?

Move a plan from `docs/plans` to `docs/plans/completed` when the planned work is finished or when the repository state clearly reflects that the plan is complete.
Before moving it, update the document so it accurately reflects what was implemented, what was verified, and whether any follow-up work remains outside the scope of that plan.

# What should be avoided in plans?

Do not leave verification vague.
Do not omit assumptions when they materially affect interpretation of the plan.
Do not keep stale status information after the implementation has moved on.
Do not leave completed plans in the active plans directory.
