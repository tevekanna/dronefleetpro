# Frontend E2E tests (Playwright)

End-to-end tests that drive the real React app against the real backend.
Designed for the docker-compose dev stack — Playwright does NOT spin up its
own servers.

Covered flows: aircraft dedupe-merge (`dedupe-merge.spec.ts`), real UI login
(`auth-login.spec.ts`), airworthiness status change (`airworthiness.spec.ts`),
and defect reporting (`defect-report.spec.ts`).

## Prerequisites

1. Dev stack is running:

   ```
   docker compose -f docker-compose.dev.yml up -d
   ```

2. Backend healthcheck returns 200:

   ```
   curl http://localhost:3000/api/v1/health
   ```

3. The default admin password has not been rotated (or you provide overrides;
   see "Credentials" below).

4. Docker CLI is on `PATH` and the `db` service in the dev stack is up. The
   test-data fixture uses `docker compose exec db psql ...` to flip the
   `needs_manual_sn` flag, because the backend's create-aircraft API does
   not expose it (only the QR-scan and batch-import paths set it internally).

## First-time setup

```
cd frontend
npm install
npm run test:e2e:install
```

The `:install` script downloads only the Chromium runtime — Webkit and
Firefox are not installed.

## Running tests

```
# Headless run (terminal output)
npm run test:e2e

# Interactive UI mode — useful for debugging selectors
npm run test:e2e:ui
```

Reports and traces land in `frontend/playwright-report/` and
`frontend/test-results/` (both gitignored).

## Credentials

The auth fixture logs in as the default admin. Override via env vars or a
`frontend/.env.local` file (also gitignored):

```
E2E_ADMIN_EMAIL=admin@dronefleetpro.local
E2E_ADMIN_PASSWORD=dronefleet2025
```

If the password has been rotated and the override is missing, the auth
fixture throws with a clear message before any test runs.

## URLs

| Var                  | Default                          | Notes                                                                                  |
|----------------------|----------------------------------|----------------------------------------------------------------------------------------|
| `E2E_FRONTEND_URL`   | `http://localhost:5173`         | Origin only — specs use root-relative paths (Vite is configured with `base: '/'`). |
| `E2E_BACKEND_URL`    | `http://localhost:3000`         | Direct backend port — bypasses the Vite proxy, which targets the docker-network host. |
| `E2E_COMPOSE_FILE`   | `../docker-compose.dev.yml`     | Path from `frontend/` to the compose file used by the test-data fixture's psql calls. |

## Test data convention

- `equipment_id` follows the pattern `UAV-e2e-*` with a `Date.now()` suffix
  so each run is unique even if a previous run leaked rows.
- The merge endpoint hard-deletes the source aircraft, so the pending row
  cleans itself up on a successful test.
- Target rows are explicitly cleaned in `afterAll`. On a test crash they may
  leak, leaving `UAV-e2e-target-*` rows in the fleet list.

## Cleaning up leaked test data

```
docker compose -f docker-compose.dev.yml exec db psql -U dronefleet -d dronefleet \
  -c "DELETE FROM aircraft WHERE equipment_id LIKE 'UAV-e2e-%';"
```

⚠ This bypasses the audit log. Only run on a dev database.

## Troubleshooting

- **Connection refused on port 5173 or 3000** — the dev stack isn't up; run
  `docker compose -f docker-compose.dev.yml ps` and bring it up if needed.
- **`Admin login failed: 401`** — the default password has been rotated.
  Set `E2E_ADMIN_PASSWORD` to the current one.
- **Test data cleanup fails** — `docker` isn't on `PATH`, the compose file
  path is wrong, or the `db` service is down. The cleanup helper logs a
  warning but won't fail the run; clean leaked rows with the one-liner
  above.
- **Selectors fail** — run `npm run test:e2e:ui` to step through the test
  interactively, inspect the DOM, and adjust selectors.
