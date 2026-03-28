# What is this plan for?

This plan defines how to create a new web UI in `./ui` using `React`, `pnpm`, `Vite`, `Vitest`, and `Ant Design`.
The goal is to replace the current inline HTML dashboard with a frontend that is fast to iterate on, pleasant to extend, and easy for the Rust web layer to serve in production.

# What problem is this plan solving?

`cid` already has a minimal web surface in [`crates/web`](/data/projects/cid/crates/web), but that surface is currently a hand-built HTML response with YAML endpoints.

That is good enough to prove the product shape, but it is a poor foundation for the UI the project actually wants:

- richer repository and run views
- clearer status presentation
- reusable layout and component structure
- frontend tests for rendering and interaction behavior
- a sane path for local UI development

Without an explicit plan, the frontend is likely to drift into one of two bad outcomes:

- a Vite app that is easy to run in development but awkward to integrate into the Rust server
- a server-coupled UI that is hard to test and miserable to iterate on

Both are avoidable.

# What should this first UI setup optimize for?

The first setup should optimize for:

- a boring, standard frontend toolchain
- fast local iteration with `pnpm` and Vite
- clear integration points with the existing Rust web crate
- testable UI code with `Vitest`
- a visual system that looks credible quickly with `Ant Design`

The first setup should not optimize for:

- server-side rendering
- micro-frontends
- a complicated state-management stack
- bespoke design tokens before real screens exist
- premature charting or animation libraries

# How should the new UI coexist with the current Rust web layer?

The simplest correct split is:

- `./ui` owns the React application, static assets, frontend tests, and build pipeline
- `crates/web` owns the HTTP server, API routes, and production asset serving

Recommended near-term shape:

- keep `crates/web` responsible for `/api/*`
- move the current dashboard HTML out of the request path once the React app is ready
- serve the built `./ui/dist` assets from `crates/web` in production
- use the Vite dev server during frontend development instead of trying to hot-reload through Rust

This keeps the runtime boundary obvious.
The Rust server is the backend and asset host.
The React app is the client.

# What API shape should the frontend target first?

The current YAML endpoints are useful for debugging, but they are a bad frontend contract.
The UI bootstrap should introduce JSON-first API responses for frontend-facing routes.

Recommended initial API targets:

- `GET /api/repositories`
- `GET /api/runs`
- `GET /api/runs/:id`
- optionally `GET /api/summary` if the dashboard wants aggregate counters without recomputing them client-side

The payloads should stay small, explicit, and stable enough for TypeScript typing.

The frontend should not parse YAML in the browser just because the current server happens to emit it.
That would be the wrong kind of clever.

# What should the initial UI architecture look like?

The initial frontend should stay intentionally small.

Recommended structure inside `./ui`:

- `src/main.tsx` for bootstrapping
- `src/app/` for app shell, routing, providers, and shared layout
- `src/features/repositories/` for repository list and status views
- `src/features/runs/` for run list and run detail views
- `src/lib/api/` for fetch wrappers and response typing
- `src/lib/test/` for shared test setup utilities
- `src/styles/` for theme overrides and global CSS

Recommended library choices for the first cut:

- `@tanstack/react-router` for fileless, type-safe routing
- native `fetch` with a thin wrapper instead of a full data library
- `Ant Design` components plus a small local theme layer

Avoid adding Redux, Zustand, React Query, or a custom design system on day one unless a real screen forces the need.

# What should the first user-facing screens include?

The first UI iteration should include only the screens needed to replace the current dashboard with something materially better:

- a dashboard or home view with repository and run summary cards
- a recent runs table
- a run detail view with step status and high-value metadata
- an empty state for repositories or runs when no data exists yet
- basic loading and error states

That is enough to validate layout, routing, data fetching, and design-system usage without getting lost in polish work.

# How should Ant Design be used?

Ant Design should be used as a baseline system, not as an excuse to ship the default demo look untouched.

The setup should include:

- a shared `ConfigProvider`
- a small theme definition for colors, spacing, and border radius
- a consistent page shell with header, content width, and status treatments
- a limited component vocabulary for tables, tags, cards, empty states, alerts, and descriptions

The first pass should not try to override every token.
Pick a clear visual direction, make the dashboard readable, and keep the theme easy to adjust.

# How should development and production workflows work?

Recommended workflow:

1. `pnpm` manages the `./ui` workspace
2. `pnpm dev` runs the Vite dev server for frontend iteration
3. the Vite dev server proxies `/api/*` requests to the local Rust web server
4. `pnpm build` produces `./ui/dist`
5. `crates/web` serves `./ui/dist` in production mode

This gives the frontend a normal developer experience without coupling it to backend rebuilds for every CSS or component change.

# In what order should the work happen?

Recommended order:

1. create the `./ui` project with `pnpm`, Vite, React, and TypeScript
2. add `Vitest` and shared test setup
3. add `Ant Design`, `@tanstack/react-router`, and the app shell
4. define JSON API contracts in `crates/web`
5. add a Vite dev proxy for `/api/*`
6. build the dashboard and recent runs views
7. build the run detail view
8. teach `crates/web` to serve built frontend assets from `./ui/dist`
9. remove or retire the inline HTML dashboard path once the React UI replaces it cleanly

This order keeps the frontend unblocked while still forcing backend contract cleanup early.

# What assumptions should be made explicit?

This plan assumes:

- `./ui` is intentionally outside the Rust workspace
- `pnpm` is the only supported package manager for the frontend
- TypeScript is acceptable even though the user asked for React rather than explicitly for TS
- the Rust server will remain the production host for API and static assets
- the first UI can rely on request-response fetching rather than live streaming

If the project later wants websocket updates, SSR, or a standalone frontend deployment, the plan should be revised rather than stretched.

# What are the main risks?

The main risks are:

- keeping the existing YAML API too long and creating frontend-specific parsing hacks
- coupling frontend routes too tightly to server-side file layout
- allowing Ant Design defaults to define the product’s entire visual identity
- adding too much frontend infrastructure before the first real screens exist
- making production asset serving an afterthought and discovering deployment friction late

# How should this work be tracked?

- [ ] Create `./ui/package.json` with `pnpm` scripts for `dev`, `build`, `test`, and `test:run`
- [ ] Scaffold the Vite React app in `./ui`
- [ ] Add TypeScript configuration for the Vite app
- [ ] Add `Vitest`, `jsdom`, and frontend test setup files
- [ ] Add `Ant Design` and a shared `ConfigProvider` theme wrapper
- [ ] Add `@tanstack/react-router` and a minimal application route structure
- [ ] Add a frontend API client layer with typed JSON responses
- [ ] Add JSON API responses in `crates/web` for repositories, runs, and run detail
- [ ] Add Vite dev proxy configuration for `/api/*`
- [ ] Implement the initial dashboard and recent runs views
- [ ] Implement the initial run detail view
- [ ] Add empty, loading, and error states for the first screens
- [ ] Add colocated frontend tests for the app shell, routing, and at least one data-backed screen
- [ ] Add production serving for `./ui/dist` in `crates/web`
- [ ] Remove or retire the inline HTML dashboard once parity is reached
- [ ] Run `pnpm test:run` in `./ui`
- [ ] Run `pnpm build` in `./ui`
- [ ] Run `./scripts/check-code.sh`

# How should the work be verified?

Verification should include:

- frontend unit or component tests with `Vitest`
- a production build of the Vite app
- manual verification that the Vite dev server can reach the Rust `/api/*` endpoints through proxying
- manual verification that `crates/web` can serve the built frontend assets
- repository-wide verification through `./scripts/check-code.sh`

If the repository-wide check does not yet cover the frontend toolchain, that gap should be called out explicitly and closed in follow-up work instead of being ignored.
