# What is this plan for?

This plan defines how to add repository-focused UI screens to `./ui`.
The goal is to let users open a repository page, see branches ordered by most recent build activity with clear build status, drill into one branch, and deep-link to those screens through TanStack Router.

# What problem is this plan solving?

The current UI gives a dashboard and individual run detail pages, but it is still missing a useful middle layer between "all repositories" and "one run."

That leaves a few obvious product gaps:

- there is no repository detail page
- there is no branch-centric view for a repository
- there is no way to answer "what happened to `main` lately?" without mentally filtering the global runs table
- there are no stable deep links for repository and branch views

That is awkward because branch health is one of the main things a CI UI should make boringly easy to see.

# What is the current status?

The repository already includes:

- a Vite + React + TypeScript frontend in `./ui`
- TanStack Router with routes for `/` and `/runs/$runId`
- JSON endpoints for repositories, runs, run detail, and summary
- a dashboard that shows repositories and recent runs

What is missing is repository-level navigation and any API shape that groups branch history by repository.

# What should this implementation optimize for?

This work should optimize for:

- quick branch-health scanning within one repository
- stable deep links for repository and branch views
- a small, explicit backend contract instead of frontend-side guesswork
- reuse of existing run status styling and table patterns
- straightforward extension from "latest branch status" to "branch run history"

This work should not optimize for:

- infinite filtering controls on day one
- speculative charts or commit graphs
- a generalized query layer
- server-side pagination unless real data volume forces it

# What should the repository UI show?

The repository page should show branches for one repository in last-build order.

Each branch row or card should include at least:

- branch name
- latest build status
- latest commit sha or short commit
- latest queued or finished timestamp
- a link into the branch detail view

"Last build order" should mean ordering branches by the newest relevant run timestamp, not alphabetically.
If a branch has never run, it should still appear if it is configured, but it should sort after branches with recorded runs.

# What should the branch UI show?

The branch page should show the runs for one branch within one repository.

Each run entry should include at least:

- run id with link to `/runs/$runId`
- status
- commit sha
- queued time
- started or finished time when available

The branch page should answer "what has happened recently on this branch?" without forcing the user back to the global dashboard.

# What route structure should be added?

TanStack Router should own deep-linkable repository and branch routes.

Recommended route shape:

- `/repositories/$repositoryId`
- `/repositories/$repositoryId/branches/$branchName`

That keeps repository identity and branch identity explicit in the URL and makes linking from the dashboard straightforward.

The implementation should avoid encoding branch state only in search params.
These screens are primary navigation targets, not ephemeral filters.

# What backend contract should the UI use?

The current endpoints are too flat for this feature.
The frontend could derive some of the grouping client-side from `/api/runs`, but that would be the wrong tradeoff:

- it repeats grouping logic in the browser
- it makes repository pages download more data than they need
- it leaves route-specific loading states tied to unrelated global payloads

Recommended API additions:

- `GET /api/repositories/:id`
  Returns repository metadata needed for the repository page.
- `GET /api/repositories/:id/branches`
  Returns one summary row per branch, already ordered by most recent build activity.
- `GET /api/repositories/:id/branches/:branch`
  Returns the branch summary plus recent runs for that branch.

The branch summary payload should include configured branches even when they have no runs yet.
That avoids turning "never built" into "branch silently missing."

# What frontend structure should be introduced?

Recommended additions inside `./ui`:

- `src/features/repositories/repository-page.tsx`
- `src/features/repositories/repository-page.test.tsx`
- `src/features/repositories/branch-page.tsx`
- `src/features/repositories/branch-page.test.tsx`
- `src/features/repositories/status.tsx` or a similarly small helper if status rendering starts repeating
- route definitions in `src/app/router.tsx`
- typed API helpers and response types in `src/lib/api/`

This should stay boring.
If a tiny shared helper improves readability, use it.
If the code starts inventing abstractions for every table cell, stop.

# How should the dashboard connect to the new views?

The dashboard should become the entry point into repository-specific navigation, not a dead-end summary page.

Recommended changes:

- repository cards on `/` should link to `/repositories/$repositoryId`
- branch tags or latest-branch affordances may link directly to branch pages when useful
- recent runs should keep linking to `/runs/$runId`

This keeps the navigation model coherent:

- dashboard for overview
- repository page for branch health
- branch page for branch history
- run page for execution detail

# What assumptions should be made explicit?

This plan assumes:

- repository ids are stable enough to use in route params
- branch names can be safely round-tripped through route params when URL-encoded
- branch summary ordering can be computed server-side from existing run metadata
- the first branch page can show recent runs without pagination

If branch names or repository ids need different external identifiers later, the route design should be revised before it hardens into public links.

# What are the main risks?

The main risks are:

- deriving branch summaries inconsistently between backend and frontend
- mishandling URL encoding for branch names that contain slashes
- sorting by the wrong timestamp and making "last build order" misleading
- dropping configured-but-never-run branches from the repository page
- overbuilding the UI before the backend contract is stable

# In what order should the work happen?

Recommended order:

1. define the repository and branch API response shapes in `crates/web`
2. add repository and branch endpoints in `crates/web`
3. add typed API helpers and response types in `./ui`
4. add TanStack Router routes for repository and branch pages
5. implement the repository page with branches ordered by most recent build activity
6. implement the branch page with branch-specific run history
7. update the dashboard to link into the new repository and branch routes
8. add frontend tests for repository navigation, branch loading, and deep-link rendering
9. run frontend and repository-wide verification

This order keeps the routing and UI grounded in a real backend contract instead of mock-shaped wishful thinking.

# How should this work be tracked?

- [ ] Add JSON API response types for repository detail and branch summary payloads
- [ ] Add `GET /api/repositories/:id`
- [ ] Add `GET /api/repositories/:id/branches`
- [ ] Add `GET /api/repositories/:id/branches/:branch`
- [ ] Make branch summary ordering reflect most recent build activity
- [ ] Include configured branches with no runs in the repository branch list
- [ ] Add frontend API client helpers for repository and branch routes
- [ ] Add TanStack Router routes for `/repositories/$repositoryId`
- [ ] Add TanStack Router routes for `/repositories/$repositoryId/branches/$branchName`
- [ ] Implement the repository page UI with branch status and last-build ordering
- [ ] Implement the branch page UI with branch-specific run history
- [ ] Update dashboard links to point at repository and branch deep links
- [ ] Add colocated frontend tests for repository page rendering and branch page deep links
- [ ] Add backend tests for repository and branch JSON responses
- [ ] Run `pnpm --dir ui test:run`
- [ ] Run `pnpm --dir ui build`
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- backend tests for repository detail and branch endpoints
- frontend tests that render the repository page from mocked API responses
- frontend tests that render the branch page from a deep-linked route
- verification that branch routes handle URL-encoded branch names correctly
- a production frontend build through `pnpm --dir ui build`
- repository-wide verification through `./scripts/check-code.sh`

Manual smoke checking is also worth doing once the feature lands:

- open a repository page from the dashboard
- confirm branches are ordered by latest build activity
- open a branch page from that repository page
- confirm branch runs match the expected status history
