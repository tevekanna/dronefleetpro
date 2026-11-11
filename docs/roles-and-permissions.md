# Roles and Permissions Reference

**Generated:** 2026-04-21
**Source of truth:** the Rust code in `backend/src/`. Where this document
contradicts the internal design notes, the code is authoritative; discrepancies are
flagged in §4.

"Internal design notes" in this document refers to a maintainer document
that is not part of the published repository; section numbers are kept for
the maintainers' reference.

**Methodology.** Walked every route in [backend/src/main.rs](../backend/src/main.rs),
traced each into its handler, and grepped the handler tree for role-check
patterns (`claims.role`, `Forbidden`, `require_*`). Total endpoints
audited: **63**. Only **four** handler files perform any role check:
[users.rs](../backend/src/handlers/users.rs),
[purge.rs](../backend/src/handlers/purge.rs),
[maintenance_schedules.rs](../backend/src/handlers/maintenance_schedules.rs),
and [maintenance_schedule_actions.rs](../backend/src/handlers/maintenance_schedule_actions.rs).
Every other endpoint protected by the `Claims` extractor is
**authenticated-any** regardless of role.

---

## 1. Role definitions

The `VALID_ROLES` constant in [users.rs:13](../backend/src/handlers/users.rs:13)
is the authoritative list:

| Role | Defined in code? | Enforced anywhere in code? |
|---|---|---|
| `admin` | yes | yes — required for user management, schedule delete, all purge endpoints |
| `technician` | yes | yes — alongside `admin` for schedule create/update/complete |
| `pilot` | yes (valid role) | **no endpoint checks for this role** |
| `safety_officer` | yes (valid role) | **no endpoint checks for this role** |
| `viewer` | yes (valid role) | **no endpoint checks for this role** |

- **`admin`** — Full power. The only role with role-check enforcement across
  multiple domains: user management, maintenance-schedule delete, every
  data-purge operation. In practice, because most endpoints don't check
  role at all, the other roles are effectively near-admin on all non-admin
  resources.
- **`technician`** — Recognised alongside `admin` by the
  `require_admin_or_technician` guard in
  [maintenance_schedules.rs:30](../backend/src/handlers/maintenance_schedules.rs:30)
  for creating/updating/completing maintenance schedules. Note that plain
  maintenance *records* (not schedules) are not role-gated at all.
- **`pilot`** — A valid role but never checked. Any authenticated user can
  currently log flights, import logs, and submit EASA debriefs.
- **`safety_officer`** — A valid role but never checked. The internal design notes §7
  describes this as the incident-owning role; no such gating exists in
  code.
- **`viewer`** — A valid role but never checked. The internal design notes §7 describes
  this as read-only; in practice, a `viewer` has write access to every
  endpoint except the four gated ones below.

> **Naming.** The task brief calls the read-only role `observer`, but the
> code and the internal design notes §7 both call it `viewer`. There is no `observer` role
> in the system.

> **Login gate.** [auth.rs](../backend/src/handlers/auth.rs) rejects
> logins for users where `is_active = FALSE`, so deactivated users cannot
> obtain a token regardless of their role.

---

## 2. Permissions matrix

Legend — **Min role**:
- `public` — no auth required
- `auth` — any authenticated user (any role, including `viewer`)
- `technician+` — admin or technician
- `admin` — admin only

### Auth
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| POST | `/api/v1/auth/login` | public | Issue JWT | Verifies `is_active` |
| POST | `/api/v1/auth/refresh` | public | (stub) | Takes no `Claims` |
| POST | `/api/v1/auth/logout` | public | (stub) | Takes no `Claims` |
| GET  | `/api/v1/auth/me` | auth | Current user profile | |

### Health
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET  | `/api/v1/health` | public | Version + build info | Added in recent release |

### Dashboard
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET  | `/api/v1/dashboard` | auth | Aggregate widgets | Org-scoped |

### Flights
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/flights` | auth | List flights | Org-scoped, filterable |
| POST   | `/api/v1/flights` | auth | Create manual flight | No role check |
| GET    | `/api/v1/flights/stats` | auth | Aggregate stats | |
| POST   | `/api/v1/flights/import` | auth | Parse DJI/Litchi log (preview) | No role check |
| POST   | `/api/v1/flights/commit` | auth | Commit imported flight | No role check |
| GET    | `/api/v1/flights/:id` | auth | Fetch flight | |
| PUT    | `/api/v1/flights/:id` | auth | Update flight | No role check |
| DELETE | `/api/v1/flights/:id` | auth | Soft-delete flight | **No role check — see §4** |

### Aircraft
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/aircraft` | auth | List aircraft | Org-scoped |
| POST   | `/api/v1/aircraft` | auth | Create aircraft | No role check |
| GET    | `/api/v1/aircraft/:id` | auth | Fetch aircraft | |
| PUT    | `/api/v1/aircraft/:id` | auth | Update aircraft | No role check |
| DELETE | `/api/v1/aircraft/:id` | auth | Retire aircraft (status flip) | No role check |

### Batteries
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/batteries` | auth | List batteries | |
| POST   | `/api/v1/batteries` | auth | Create battery | No role check |
| GET    | `/api/v1/batteries/:id` | auth | Fetch battery | |
| PUT    | `/api/v1/batteries/:id` | auth | Update battery | No role check |
| DELETE | `/api/v1/batteries/:id` | auth | Retire battery | No role check |

### Controllers
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/controllers` | auth | List controllers | |
| POST   | `/api/v1/controllers` | auth | Create controller | No role check |
| GET    | `/api/v1/controllers/:id` | auth | Fetch controller | |
| PUT    | `/api/v1/controllers/:id` | auth | Update controller | No role check |
| DELETE | `/api/v1/controllers/:id` | auth | Retire controller | No role check |

### Fleet Batch Import
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| POST | `/api/v1/fleet/batch-import` | auth | Preview CSV batch (aircraft/battery/controller) | No role check |
| POST | `/api/v1/fleet/batch-commit` | auth | Commit batch | No role check |

### Maintenance Records
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/maintenance` | auth | List records | Org-scoped |
| POST   | `/api/v1/maintenance` | auth | Create maintenance record (OSO #3) | **No role check — see §4** |
| GET    | `/api/v1/maintenance/:id` | auth | Fetch record | |
| PUT    | `/api/v1/maintenance/:id` | auth | Update record | No role check |
| DELETE | `/api/v1/maintenance/:id` | auth | Delete record | No role check |

### Maintenance Schedules
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/maintenance-schedules` | auth | List schedules | |
| POST   | `/api/v1/maintenance-schedules` | **technician+** | Create schedule | [maintenance_schedules.rs:128](../backend/src/handlers/maintenance_schedules.rs:128) |
| GET    | `/api/v1/maintenance-schedules/dashboard` | auth | Schedule status dashboard | |
| GET    | `/api/v1/maintenance-schedules/:id` | auth | Fetch schedule | |
| PUT    | `/api/v1/maintenance-schedules/:id` | **technician+** | Update schedule | [maintenance_schedules.rs:224](../backend/src/handlers/maintenance_schedules.rs:224) |
| DELETE | `/api/v1/maintenance-schedules/:id` | **admin** | Delete schedule | [maintenance_schedules.rs:384](../backend/src/handlers/maintenance_schedules.rs:384) |
| POST   | `/api/v1/maintenance-schedules/:id/complete` | **technician+** | Mark completed + emit maintenance record | [maintenance_schedule_actions.rs:43](../backend/src/handlers/maintenance_schedule_actions.rs:43) |

### Defects
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/defects` | auth | List defects | |
| POST   | `/api/v1/defects` | auth | Create defect | No role check |
| PUT    | `/api/v1/defects/:id` | auth | Update defect status | No role check |
| DELETE | `/api/v1/defects/:id` | auth | Delete defect | No role check |

### Pilots
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/pilots` | auth | List pilots | |
| POST   | `/api/v1/pilots` | auth | Create pilot | No role check |
| GET    | `/api/v1/pilots/:id` | auth | Fetch pilot + qualifications | |
| PUT    | `/api/v1/pilots/:id` | auth | Update pilot | No role check |
| DELETE | `/api/v1/pilots/:id` | auth | Delete pilot | No role check |

### Incidents
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/incidents` | auth | List incidents | |
| POST   | `/api/v1/incidents` | auth | Create incident | No role check |
| PUT    | `/api/v1/incidents/:id` | auth | Update incident | No role check |
| DELETE | `/api/v1/incidents/:id` | auth | Delete incident | No role check |

### Users (organisation members)
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET    | `/api/v1/users` | **admin** | List users | [users.rs:56](../backend/src/handlers/users.rs:56) |
| POST   | `/api/v1/users` | **admin** | Create user | Validates role in `VALID_ROLES` |
| PUT    | `/api/v1/users/:id` | **admin** | Update user | Self-protection: cannot change own role |
| DELETE | `/api/v1/users/:id` | **admin** | Deactivate user | Self-protection: cannot deactivate self |

### Reports
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET | `/api/v1/reports/flights` | auth | Flight log PDF | |
| GET | `/api/v1/reports/maintenance` | auth | Maintenance report PDF | |
| GET | `/api/v1/reports/pilot-logbook` | auth | Pilot logbook PDF | |
| GET | `/api/v1/reports/fleet` | auth | Fleet status PDF | |

### Admin Purge (destructive)
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| DELETE | `/api/v1/admin/purge/flights` | **admin** | Hard-delete all flights | [purge.rs:48](../backend/src/handlers/purge.rs:48) |
| DELETE | `/api/v1/admin/purge/aircraft` | **admin** | Hard-delete all aircraft | Nullifies FKs first |
| DELETE | `/api/v1/admin/purge/batteries` | **admin** | Hard-delete all batteries | |
| DELETE | `/api/v1/admin/purge/controllers` | **admin** | Hard-delete all controllers | |
| DELETE | `/api/v1/admin/purge/pilots` | **admin** | Hard-delete all pilots | |
| DELETE | `/api/v1/admin/purge/incidents` | **admin** | Hard-delete all incidents | |

### Settings
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET | `/api/v1/settings` | auth | List all org settings | |
| GET | `/api/v1/settings/:key` | auth | Fetch one setting | |
| PUT | `/api/v1/settings/:key` | auth | Upsert setting | **No role check — see §4** |

### Audit Log
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET | `/api/v1/audit-log` | auth | List audit rows | Org-scoped, paginated |
| GET | `/api/v1/audit-log/filters` | auth | Filter options | |
| GET | `/api/v1/audit-log/:id` | auth | Fetch audit row | |

### Fleet Statistics
| Method | Path | Min role | Action | Notes |
|---|---|---|---|---|
| GET | `/api/v1/fleet-stats/overview` | auth | Overview stats | |
| GET | `/api/v1/fleet-stats/export/batteries` | auth | Battery CSV export | |
| GET | `/api/v1/fleet-stats/controllers/export` | auth | Controller CSV export | |
| GET | `/api/v1/fleet-stats/batteries/model/:model` | auth | Battery model comparison | |
| GET | `/api/v1/fleet-stats/batteries/:id` | auth | Battery detail | |
| GET | `/api/v1/fleet-stats/controllers/:id` | auth | Controller detail | |

### Org (referenced in the internal design notes §6, not present in code)
The internal design notes §6 lists `GET/PUT /api/v1/org` for organisation settings.
These routes are **not implemented** in [main.rs](../backend/src/main.rs).
See §4.

---

## 3. Capability summary per role

### admin
Can do:
- Everything any other authenticated role can do (see below).
- Manage users (list, create, update role/password/pilot-link, deactivate).
- Delete maintenance schedules.
- Execute every `/api/v1/admin/purge/*` endpoint (hard-deletes all
  records of a given resource for the org).
- Create/update maintenance schedules (shared with `technician`).

Cannot do:
- Change their own role or deactivate their own user record
  ([users.rs:177, 270](../backend/src/handlers/users.rs:177)).

### technician
Can do:
- Create, update, and complete maintenance schedules.
- Every other endpoint authenticated users can hit — including writes
  to flights, aircraft, batteries, pilots, incidents, settings, etc.
  (no enforcement exists).

Cannot do:
- Manage users.
- Delete maintenance schedules.
- Execute any `/api/v1/admin/purge/*` endpoint.

### pilot
Role has no custom enforcement. In practice identical to
`safety_officer` and `viewer`:

Can do:
- Everything except user management, schedule delete, and admin purge.

Cannot do:
- Manage users.
- Delete maintenance schedules.
- Execute admin purge endpoints.
- Create/update/complete maintenance schedules (though this restriction
  only applies via the `admin_or_technician` guard, not to the pilot
  role specifically).

### safety_officer
Identical to `pilot` and `viewer` in actual capability — role is
recognised as valid for user creation/update but never checked on any
endpoint. The internal design notes §7 describes this as an incident-owning role; no
such enforcement exists in code.

### viewer
Identical to `pilot` and `safety_officer` in actual capability.
**The internal design notes §7 describes `viewer` as read-only, but in code a `viewer`
can write to every endpoint that lacks a role check** — i.e. create,
update, and delete flights, aircraft, batteries, controllers, pilots,
incidents, defects, maintenance records, and settings. This is the
most significant discrepancy between docs and code.

---

## 4. Observations and recommendations

**The enforcement surface is tiny.** Only 4 of 63 endpoints gate on
role: user CRUD, schedule delete, schedule CUD (via `admin_or_technician`),
and the six purge endpoints. Every other write endpoint is accessible to
any authenticated user regardless of role. If the production intent
matches the internal design notes §7 ("viewer — read-only … pilot — can log flights,
report defects … technician — maintenance records … safety_officer —
incident management"), then most of the product's role model is
aspirational rather than enforced. The `pilot`, `safety_officer`, and
`viewer` roles are purely cosmetic today.

**Settings endpoint is under-protected.** [settings.rs](../backend/src/handlers/settings.rs)
lets any authenticated user write to the org's settings table, including
secret keys like `dji_api_key` that are specifically scrubbed from the
audit log — meaning a `viewer` could rotate or wipe the org's DJI
developer key and leave only a `_scrubbed_changed` marker. This is the
single highest-priority gap. Add `require_admin` (or introduce a new
"manage-settings" role check) to `upsert_setting`.

**Maintenance records vs. maintenance schedules are inconsistent.**
Schedule CUD requires `admin_or_technician`, but the underlying
`/api/v1/maintenance` records they reference have no role check at all
— any authenticated user can insert or delete an EASA OSO #3
maintenance record. Either the schedules gate is too strict or the
records gate is too loose; they should match.

**Flight deletion by any role is risky.** `DELETE /api/v1/flights/:id`
is a soft delete, but it still removes the flight from normal listings
and PDF reports. A `viewer` or `pilot` should not be able to do this.
At minimum, deletion should require the pilot of record, a safety
officer, or an admin.

**`org` endpoints are documented but not implemented.** The internal design notes §6
lists `GET/PUT /api/v1/org` as admin-only; the router has no such
routes. Either remove from the doc or implement.

**Redundant `require_admin` helpers.** The exact same function body
(`if claims.role != "admin" { return Forbidden }`) is duplicated in
[users.rs:48](../backend/src/handlers/users.rs:48),
[purge.rs:17](../backend/src/handlers/purge.rs:17),
and [maintenance_schedules.rs:39](../backend/src/handlers/maintenance_schedules.rs:39).
Promote to a single helper in `auth::middleware` with a corresponding
`require_role(&claims, &["admin"])` or bitmask form, alongside the
existing `require_admin_or_technician`.

**No role-related TODOs or FIXMEs** exist in the codebase.

**Self-protection gaps.** `update` and `delete` on users correctly
prevent a user from demoting or deactivating *themselves*, but there is
no "last admin" guard — one admin can deactivate another admin. In a
two-admin org, admin A can deactivate admin B, then be deactivated by
admin C (if any), leaving the org without admins. In practice, with the
single-admin dev default, this is safe. Worth a `WHERE
(SELECT COUNT(*) FROM users WHERE org_id = $1 AND role = 'admin' AND
is_active) > 1` guard before demoting or deactivating an admin.

**No model-scoped permissions exist.** There is no code hook linking
pilot qualifications (`pilot_qualifications` table) to flight creation.
`POST /api/v1/flights` does not check whether the named pilot has a
qualification covering the aircraft's EASA category or model. This is
the intended hook point for a future "pilot must be A2/SPEC-qualified
to log this flight" gate. No placeholder function or comment currently
exists in the flight-create handler — it would have to be added from
scratch.

**Internal design notes vs. code discrepancies.**
1. The internal design notes §7 describes role behaviour that is not enforced (see
   above). Update the doc to say "currently advisory — see
   `docs/roles-and-permissions.md` for live enforcement".
2. The internal design notes §6 documents `/api/v1/org` endpoints that do not exist.
3. The internal design notes §18 (the task brief's cited section) does not exist —
   the role list is in §7.
4. The internal design notes describes the read-only role as `viewer`; the task brief
   used `observer`; the code says `viewer`.
