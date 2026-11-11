# DroneFleetPro

DroneFleetPro is a self-hosted, multi-tenant web application for managing a
professional fleet of unmanned aircraft. It keeps the records an operator
needs in one place, with a full audit trail.

> **Disclaimer.** DroneFleetPro is a record-keeping aid. It is **not**
> certified, approved or endorsed by EASA or by any national aviation
> authority. It does not provide legal or regulatory advice, and it comes with
> **no warranty** of any kind (see [LICENSE](LICENSE)). The UAS operator
> remains solely responsible for complying with all applicable regulations
> and for the correctness of its own records.

## What it does

- **Fleet** — aircraft, batteries, remote controllers and other equipment,
  with equipment IDs, status, retirement and usage statistics.
- **Flights** — a digital flight logbook with manual entry, GPS trajectories
  on a map, filters and statistics.
- **Log import** — DJI flight logs (binary `.txt` and CSV exports, including
  Litchi) and ArduPilot logs, with automatic matching of aircraft, batteries
  and controllers by serial number.
- **Pilots** — pilot profiles, qualifications and expiry tracking.
- **Maintenance** — maintenance records, defect log, recurring maintenance
  schedules per aircraft and model-level templates.
- **Batteries** — cycle counts, health indicators and estimated remaining
  life.
- **Occurrence reporting** — incident records with occurrence classification
  and reporting-deadline tracking.
- **Reports** — PDF reports for flights, maintenance, pilot logbooks and
  fleet status.
- **Audit log** — every data-changing operation is recorded in a write-once
  audit log.

### Regulatory context

DroneFleetPro is designed around operations in the EASA **Specific category**
at **SAIL I and SAIL II**, under Commission Implementing Regulation
(EU) 2019/947. Its vocabulary follows the EASA Easy Access Rules for Unmanned
Aircraft Systems, in which **SORA 2.5** is AMC1 to Article 11 and **SORA 2.0**
is retained as AMC1bis. The application helps you keep the records those
rules expect; it does not assess, decide or guarantee compliance. Always
check the current rules at the source.

## Quick start

You need Docker with Docker Compose, and a git checkout of this repository
(the backend image embeds the git commit, so a plain source archive without
the `.git` directory will not build).

### Development

```bash
docker compose -f docker-compose.dev.yml up
```

This starts PostgreSQL, the backend with hot reload on port 3000, the
frontend dev server on <http://localhost:5173> and Adminer on port 8080. The
development compose file sets a fixed first-admin password for local use;
see `DFP_BOOTSTRAP_ADMIN_PASSWORD` in `docker-compose.dev.yml`. Sign in as
`admin@dronefleetpro.local`.

### Production

```bash
cp .env.example .env
# edit .env: set DB_PASSWORD, JWT_SECRET and JWT_REFRESH_SECRET at least
docker compose up -d --build
```

The application is then served over plain HTTP on port 80. See
[TLS](#tls) before exposing it to a network you do not control.

### First admin login

A new installation contains one admin account, `admin@dronefleetpro.local`.
Its seed password is public, so the backend neutralises it on every start:

- **`DFP_BOOTSTRAP_ADMIN_PASSWORD` set** (at least 12 characters): the seed
  password is replaced with that value. Sign in with it and change it in
  Settings.
- **Not set**: the account is locked and a one-time set-password link is
  written to the backend log. Read it with `docker compose logs backend`.
  Each restart issues a new link and invalidates the previous one.

Once no account carries the seed password, the variable is ignored and can
be removed.

## Configuration

All settings are environment variables; `.env.example` documents each one.

| Variable | Required | Purpose |
|---|---|---|
| `DB_PASSWORD` | yes | Password for the PostgreSQL user. |
| `JWT_SECRET` | yes | Signing secret for access tokens (64 random characters). |
| `JWT_REFRESH_SECRET` | yes | Signing secret for refresh tokens; must differ from `JWT_SECRET`. |
| `DFP_BOOTSTRAP_ADMIN_PASSWORD` | no | First admin password; see [First admin login](#first-admin-login). |
| `DJI_API_KEY_GLOBAL` | no | Fallback DJI developer API key for decrypting DJI logs. A key can also be set per organisation in Settings. |
| `CORS_ORIGINS` | no | Comma-separated browser origins allowed to call the API from another origin. Not needed when the frontend and API share an origin, as in the provided compose file. |
| `VITE_SOURCE_CODE_URL` | no | Build-time URL of the "Source code" link; see [Licence](#licence). |
| `RATE_LIMIT_AUTH_PER_SECONDS`, `RATE_LIMIT_AUTH_BURST` | no | Rate limit on the sign-in endpoints. |
| `UPLOAD_DIR`, `RUST_LOG` | no | Upload directory and log level of the backend. |

## TLS

The provided `docker-compose.yml` serves **HTTP on port 80** and contains no
certificates. For anything other than local use, terminate TLS in front of
it: either put a reverse proxy or load balancer that handles HTTPS in front
of port 80, or supply your own TLS-enabled nginx configuration.
`nginx/nginx.conf` contains a commented example HTTPS server block to start
from.

## Map tiles

The maps load tiles from OpenStreetMap and from Esri World Imagery. Both
services have their own usage policies, which you are responsible for
following. They are intended for light use; if you expect heavy use, arrange
your own tile provider and change the tile URLs in
`frontend/src/components/map/leafletSetup.ts`.

## Licence

DroneFleetPro is licensed under the **GNU Affero General Public License,
version 3 only** (`AGPL-3.0-only`). The full text is in [LICENSE](LICENSE).
Copyright (C) 2026 Lars Wallström.

Under section 13 of the licence, if you run a **modified** version and let
users interact with it over a network, you must offer those users the
corresponding source of your version. The application shows a
"Source code (AGPL-3.0)" link in the sidebar and on the sign-in page; set
`VITE_SOURCE_CODE_URL` when building the frontend so that this link points
at your own source.

If you cannot or do not wish to meet the obligations of the AGPL, a
commercial licence is available: see
[COMMERCIAL-LICENCE.md](COMMERCIAL-LICENCE.md).

Third-party components and their licences are listed in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## Contributing, security and trademarks

- [CONTRIBUTING.md](CONTRIBUTING.md) — development setup, conventions and
  the contributor licence agreement ([CLA.md](CLA.md)).
- [SECURITY.md](SECURITY.md) — how to report a vulnerability privately.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — expected behaviour.
- [TRADEMARKS.md](TRADEMARKS.md) — use of the DroneFleetPro name and logo.
