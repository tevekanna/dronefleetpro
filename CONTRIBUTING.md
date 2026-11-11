# Contributing to DroneFleetPro

Thank you for considering a contribution. This page explains how to set up a
development environment, the conventions the code follows and the
contributor licence agreement every contribution requires.

## Before you start

- For anything larger than a small fix, open an issue first so the approach
  can be agreed before you invest time.
- Report security vulnerabilities privately; see [SECURITY.md](SECURITY.md).
- Everyone taking part is expected to follow the
  [Code of Conduct](CODE_OF_CONDUCT.md).

## Contributor licence agreement

Every contribution requires that you sign the contributor licence agreement
in [CLA.md](CLA.md). You keep the copyright in your contribution; the
agreement grants the project the rights it needs to distribute your work
under the AGPL and under other licences, including commercial ones.

Signing happens on the pull request itself: the **CLA Assistant** check asks
you to accept the agreement the first time you open a pull request, and the
check must pass before the pull request can be accepted.

## How accepted changes reach this repository

This repository is a published export of an upstream repository maintained
by the copyright holder. An accepted pull request may therefore be ported
into the upstream repository instead of being merged here directly. When
that happens, your change appears in this repository as part of a later sync
commit, with attribution to you in the commit message.

## Development setup

You need Docker with Docker Compose, and Node.js 20 if you want to run the
frontend checks outside a container.

```bash
docker compose -f docker-compose.dev.yml up
```

This starts PostgreSQL, the backend (Rust, hot reload) on port 3000 and the
frontend (Vite) on <http://localhost:5173>.

Checks to run before opening a pull request:

```bash
# backend (inside the backend container, or with a local Rust 1.88 toolchain
# and a PostgreSQL database in DATABASE_URL)
cargo build --locked
cargo test --locked

# frontend
cd frontend
npm ci
npm run typecheck
npm run lint
npm run build

# version lockstep
./scripts/check-version-lockstep.sh
```

The end-to-end suite (`npm run test:e2e` in `frontend`) needs the full
development stack running.

## Conventions

- **British English** in all code comments, documentation and user-interface
  text: colour, organisation, licence (noun), analyse, behaviour.
- **Licence header.** Every source file starts with the two-line SPDX header
  used throughout the repository. SQL migrations are the exception: applied
  migrations are never edited.
- **Backend.** Rust with Axum and SQLx runtime queries (`sqlx::query`, not
  the `query!` macros). Every query is scoped to the organisation taken from
  the authenticated token, never from the request. No `unwrap()` in
  production code. Migrations are append-only.
- **Frontend.** React with TypeScript in strict mode; no `any`.
  - Follow the **Rules of Hooks**: hooks are called unconditionally and at
    the top level of a component, before any early return.
  - Use the `IsoDateInput` and `IsoDateTimeInput` components for dates, not
    native date inputs; dates are always shown as ISO 8601.
  - Never use `window.confirm` or `window.alert`. Destructive actions are
    confirmed in a modal, and validation errors are shown inline.
  - Styling uses CSS Modules and the design tokens in
    `frontend/src/styles/tokens.css`; no CSS frameworks.
- **Commits.** Stage files explicitly by path (not `git add -A` or
  `git add .`), so that nothing unintended is committed. Use conventional
  prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- **Versioning.** The version is kept in lockstep across four files:
  `frontend/package.json`, `frontend/package-lock.json`,
  `backend/Cargo.toml` and `backend/Cargo.lock`. A pull request that changes
  behaviour bumps all four together, following semantic versioning.
- **Merging.** Pull requests are merged with merge commits, not squashed or
  rebased.
