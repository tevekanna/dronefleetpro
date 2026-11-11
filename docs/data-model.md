# DroneFleetPro v2 — Data Model and Conventions

Domain conventions and fields whose semantics are not obvious from the
schema alone. For the authoritative DDL see the migration files in
`backend/migrations/`. For per-endpoint
auth/role rules see [roles-and-permissions.md](./roles-and-permissions.md).
For the DJI/Litchi parser field-coverage gaps see
[log-parser-audit.md](./log-parser-audit.md).

"Internal design notes" in this document refers to a maintainer document
that is not part of the published repository; section numbers are kept for
the maintainers' reference.

This file documents the *meaning* and *invariants* behind the columns
the schema declares — the things a fresh contributor (human or AI)
cannot reconstruct by reading the DDL alone.

---

## Aircraft identity

The aircraft identity model has two tables working together:
`aircraft.serial_number` (the canonical identity) and the
`aircraft_identifiers` auxiliary table (every other identifier a piece
of equipment may carry). The two-table split exists because DJI's
parser ecosystem exposes serial numbers in several conflicting forms,
and operators frequently know an aircraft by an alias the QR label
does not match.

### `serial_number` (canonical)

The body SN from the QR scan label printed on the airframe. This is
the stable, operator-trusted identity field.

- Stored on `aircraft.serial_number TEXT NOT NULL`, scoped per org via
  `UNIQUE(org_id, serial_number)` (the internal design notes §5).
- Convention: 14-char alphanumeric for current DJI consumer/enterprise
  airframes; older or custom builds may differ.
- When an operator edits `serial_number` via the `PUT /aircraft/:id`
  endpoint, the UPDATE handler auto-clears `needs_manual_sn` (see
  flow below).

### `aircraft_identifiers` (auxiliary)

Many-to-one with `aircraft`. Stores every identifier other than (or in
addition to) the canonical body SN.

Schema (from the internal design notes §5, migration 025):

```sql
aircraft_identifiers (
  id            SERIAL PRIMARY KEY,
  aircraft_id   INTEGER REFERENCES aircraft(id) ON DELETE CASCADE,
  org_id        INTEGER REFERENCES organisations(id) ON DELETE CASCADE,
  identifier    TEXT NOT NULL,
  source        TEXT NOT NULL,
  observed_in   TEXT,
  notes         TEXT,
  created_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
  created_at    TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(org_id, identifier, source)
)
```

The `UNIQUE(org_id, identifier, source)` constraint is **org-wide,
not per-aircraft**. This is deliberate: the same identifier should
never map to two different aircraft in the same organisation. A
collision on insert is the system telling you "you already have an
aircraft with this SN under this source — go merge them" rather than
"that's fine, attach it to both."

The official identity-resolution priority order is documented in
The internal design notes §16 under "Aircraft identifier matching":

1. Exact match on `aircraft.serial_number`
2. Exact match on `aircraft_identifiers.identifier` (any source)
3. Prefix-tolerant match on `aircraft.serial_number` (2-char
   tolerance) — defensive plumbing against
   [dji-log-parser SN truncation](#1-char-sn-truncation)

Batteries and remote controllers do **not** consult an identifier
table — they only have their own `serial_number` column plus the
2-char prefix tolerance in `services::sn_matching`. The identifier
table is aircraft-only by design.

### Source classification

The `source` column is a free-text TEXT field constrained only by
convention. Established values, all per the internal design notes §5:

| `source` value | Meaning |
|---|---|
| `body_sn` | Canonical chassis SN, typically from the QR scan label. Mirrors `aircraft.serial_number`. |
| `fc_sn` | Flight-controller SN as reported by the DJI log binary. Often differs from `body_sn` on Matrice 600 and Inspire 1 X5 — those airframes carry an FC module whose SN is distinct from the chassis SN. |
| `alias_djigo` | Device label string written by the DJI GO 4 app. May be the operator-chosen nickname or DJI's default. |
| `alias_djifly` | Device label string written by the DJI Fly app. Same caveat as `alias_djigo`. |
| `alias_manual` | Operator-entered identifier not from a DJI source. |
| `qr_label` | Raw QR string when it differs from the parsed serial (uncommon but reserved). |
| `other` | Anything not covered above. |

When an import auto-registers a new aircraft, the flight-commit path
writes two rows best-effort (the internal design notes §16): a `body_sn` row mirroring
the new `aircraft.serial_number`, plus an `alias_djigo` or
`alias_djifly` row when the log includes a device alias and it is not
the placeholder `"choose a name for your inspire"`.

### `needs_manual_sn` flow

`aircraft.needs_manual_sn` (BOOLEAN) marks an aircraft as
auto-registered by the DJI-log import path with an SN that the
operator has not yet confirmed.

The flag is set TRUE when:

- DJI log auto-register creates a new aircraft because the log SN
  does not match any existing `aircraft.serial_number` (exact or
  prefix-tolerant) and does not match any row in
  `aircraft_identifiers`.

In this state, the `aircraft.serial_number` column is populated with
whatever the DJI log reported — typically an FC SN, sometimes a
DJI-truncated value. The aircraft is functionally usable (flights
attach to it correctly because the import wrote both the row and
the matching identifier) but the QR-label body SN has not yet been
recorded against it.

The flag is auto-cleared when:

- The operator edits `aircraft.serial_number` via
  `PUT /api/v1/aircraft/:id` (the UPDATE handler clears the flag in
  the same transaction). The assumption is: an explicit operator
  edit of the canonical SN is the operator confirming the identity.
- The aircraft is merged into another via
  `POST /api/v1/aircraft/:source_id/merge` (PR #69) — the source row
  is deleted and the flag concern moves with it.

Two resolution paths for a `needs_manual_sn = TRUE` aircraft:

1. **Manual SN edit.** Operator scans the QR label, edits the
   aircraft's `serial_number` field, save. Flag clears.
2. **Merge into existing aircraft.** If the operator recognises the
   pending-review row as a duplicate of an aircraft they already
   have, use the merge endpoint (admin-only). All flights, the
   controller binding, maintenance schedules, and identifiers move
   to the canonical aircraft; the source row is deleted; the audit
   log records the merge with `reason = "manual-merge"`.

### Discovery UX

Aircraft with `needs_manual_sn = TRUE` are surfaced in the Fleet UI
(PR #68): per-accordion pending-review badge plus a "Show pending
only" filter toggle. The merge UI (PR #70) is reachable from the
EditAircraftModal for any aircraft in this state.

---

## DJI log parser quirks

These are domain quirks that affect identity resolution and import
correctness. The parser field-coverage gaps (what we extract vs what
we ignore) live separately in
[log-parser-audit.md](./log-parser-audit.md).

### FC SN vs body SN mismatch

For DJI Matrice 600 and Inspire 1 X5, `dji-log-parser` returns the
flight-controller SN (10-char numeric, e.g. `0410000001`) from the
binary log, not the chassis body SN (14-char alphanumeric from the
QR label, e.g. `W13TEST0000001`). The two are structurally different
and there is no derivation function — they have to be linked.

Verified during a dedupe of duplicate aircraft on 2026-05-16. The `fc_sn` source classification
was introduced for exactly this case; the merge SQL reclassifies the
auto-registered `body_sn` identifier as `fc_sn` when collapsing the
duplicate aircraft, then backfills a true `body_sn` row with the
QR-label value.

Practical consequence: a fresh import of a Matrice 600 or Inspire 1
X5 will produce an aircraft whose `serial_number` is the FC SN, with
`needs_manual_sn = TRUE`. The operator is expected to either edit
the SN to the QR-label body SN (which moves the FC SN into the
identifier table) or merge into an existing aircraft.

### 1-char SN truncation

The `dji-log-parser = "0.5"` crate occasionally returns SNs one
character shorter than the QR truth. Confirmed for Mavic Pro
batteries (QR: 14 chars `093TEST0000069`, log: 13 chars
`093TEST000006`). Suspected for aircraft and controllers — not
directly observed yet (the internal design notes §16).

Worked around by a 2-char prefix-tolerance fallback in
`services/sn_matching::find_by_serial`: exact match first, then
prefix match. Ambiguous prefix matches (≥2 candidates) are treated
as no-match and logged under `tracing::target = "sn_matching"`, so
the operator can decide rather than the system guessing.

If a future version of the crate exposes the full SN, the fallback
can stay as defensive plumbing; the 2-char tolerance is small
enough that false positives are unlikely.

### Binary log version differences

The parser handles binary log version 9 (unencrypted) and 13+
(encrypted, requires DJI API keychain). See the internal design notes §9 for the
field-by-field behaviour. Particularly notable:

- **Home point** prefers `FrameHome.latitude/longitude` for binary
  logs when strict WGS84 validity passes, iterating frames to skip
  the raw-integer sentinels Mavic 3 v14 sometimes emits in early
  frames. Falls back to the first valid GPS point.
- **Photo/video counts** prefer `DJILog.details.capture_num` and
  `video_time`, with a sanity guard falling back to per-frame flag
  counting when `video_time > duration_seconds + 2` (Mavic 3 / Neo
  firmwares occasionally emit cumulative lifetime counters instead
  of per-flight values).
- **DJI Fly split datetime** composes `CUSTOM.date [local]` +
  `CUSTOM.updateTime [local]` into a single ISO-8601 timestamp at
  parse time. No timezone info is in the log; the composed value
  is tagged UTC. A `tracing::warn!` fires once per import so bad
  offsets are visible in production logs. The TODO to add a
  per-org timezone setting is captured in the internal design notes §9.

### Model detection at preview time

Preview-time model resolution priority (the internal design notes §9), populated
into `FlightPreview.model_detection_status`:

1. `from_aircraft_db` — aircraft with this SN exists in the org; its
   `aircraft_models.name` is used.
2. `from_mapping` — `drone_type_mappings` row exists for
   `(org_id, app_source, drone_type_code)`.
3. `from_recover` — log's `recover.aircraft_name` is used (cleaned
   via `clean_aircraft_model`).
4. `unknown` — none of the above resolved.

At commit time, auto-registering an aircraft inserts a new
`aircraft_models` row via
`services::aircraft_models::resolve_or_create` if the name does not
already exist. This is the **only** path where catalog
auto-creation is allowed — every other endpoint rejects unknown
model names with 400.

---

## Counter denormalization

### Why denormalized

Three aggregate columns live directly on `aircraft` for read
performance:

- `total_flights INTEGER DEFAULT 0`
- `total_seconds INTEGER DEFAULT 0`
- `last_flight TIMESTAMPTZ`

Without these, the fleet list and per-aircraft detail pages would
need an aggregate JOIN-and-GROUP across the entire `flights` table
on every render. With them, those pages are simple `SELECT`s.

PR #51 (the flight CREATE path) and PR #64 (the flight DELETE path)
made the denormalized columns transactionally consistent: every
INSERT into `flights` and every soft-delete of a `flights` row
updates the parent aircraft's counters in the same transaction.
Drift was zero at last verification (2026-05-16).

The merge endpoint (PR #69) and the legacy merge SQL script
(not published) both **recompute** these
counters from scratch on the canonical aircraft after moving
flights, rather than incrementing. The recompute is cheap and
sidesteps any double-counting risk.

### How to recompute (canonical pattern)

The pattern, lifted directly from step 6 of the legacy merge script:

```sql
UPDATE aircraft SET
    total_flights = (SELECT COUNT(*) FROM flights
                      WHERE aircraft_id = $1 AND deleted_at IS NULL),
    total_seconds = COALESCE((SELECT SUM(duration_seconds) FROM flights
                               WHERE aircraft_id = $1 AND deleted_at IS NULL), 0),
    last_flight   = (SELECT MAX(flight_datetime) FROM flights
                      WHERE aircraft_id = $1 AND deleted_at IS NULL)
 WHERE id = $1;
```

The `deleted_at IS NULL` filter is required: `flights` are
soft-deleted (the internal design notes §15), and counters reflect live flights only.

---

## Audit log

### Immutable rows

Two BEFORE triggers on `audit_log` (migration 009, the internal design notes §5)
raise exceptions on any attempt to UPDATE or DELETE an existing
audit row. The triggers are intentional write-once enforcement for
EASA 4.2.3 — the audit log is meant to be tamper-evident, and
making the immutability a DB-level invariant rather than a
convention prevents application-bug-induced corruption.

Discovered (the hard way) during 2026-05-16 dedupe: the merge SQL
initially tried to amend an existing audit row to add merge
context; the triggers refused. The script was rewritten to write
new DELETE-action audit rows with the merge metadata in
`diff_jsonb`, which is the correct shape anyway.

### `diff_jsonb` structure

Conventions for what goes in `diff_jsonb`:

- **UPDATE** rows: `{ "before": {...}, "after": {...} }`. Standard
  audit shape, written by the per-row helpers in
  `backend/src/services/audit.rs`.
- **DELETE** rows: usually `{ "before": {...} }` capturing the
  pre-delete state. Some delete paths add operational metadata —
  see merge below.
- **INSERT** rows: usually `{ "after": {...} }`.
- **Bulk updates** (cascading FK nullify, mass-recompute): a
  summary row of shape
  `{ "bulk_update": { "column": ..., "set_to": ..., "row_count": ... } }`
  written once per affected table per transaction, via
  `audit::log_bulk_update`. Used by the purge handlers and any
  future path that touches many rows in one statement.
  (the internal design notes §10 — cascading deletes and bulk updates.)
- **Merge** (PR #69 endpoint, and the legacy SQL): a DELETE row on
  the source aircraft, with `diff_jsonb` carrying:

  ```json
  {
    "before": { /* full pre-delete row */ },
    "reason": "dedupe-merge",          // or "manual-merge"
    "merged_into_aircraft_id": 49,
    "merged_into_equipment_id": "UAV-049",
    "flights_moved": 3,
    "controller_moved": 1,
    "maintenance_schedules_moved": 3,
    "note": "FC SN ... reclassified body_sn -> fc_sn on canonical aircraft"
  }
  ```

  The `reason` field distinguishes the path: `dedupe-merge` for the
  2026-05-16 SQL-driven cleanup, `manual-merge` for operator-driven
  merges via the admin endpoint.

### Audit scrubbing

The audit pipeline scrubs known secret keys before writing
`diff_jsonb`. Settings keys like `dji_api_key` and password fields
on users surface as `"_scrubbed_changed"` markers instead of
plaintext. (The internal design notes §13 "Phase 7 — user management" describes
this; the internal design notes §6 settings endpoint says "secret keys
(dji_api_key) scrubbed in diff".)

### Searching the audit

Common queries, run with `psql` against the database:

```sql
-- By action and table
SELECT id, user_code, created_at, record_id
  FROM audit_log
 WHERE action = 'DELETE'
   AND table_name = 'aircraft'
   AND org_id = 1
 ORDER BY created_at DESC LIMIT 50;

-- By merge reason
SELECT id, user_code, record_id,
       diff_jsonb->>'merged_into_equipment_id' AS merged_into,
       created_at
  FROM audit_log
 WHERE diff_jsonb->>'reason' IN ('dedupe-merge', 'manual-merge')
 ORDER BY created_at DESC;

-- By user
SELECT id, action, table_name, record_id, created_at
  FROM audit_log
 WHERE user_code = 'LAR' AND org_id = 1
 ORDER BY created_at DESC LIMIT 100;

-- By date range
SELECT id, user_code, action, table_name, record_id
  FROM audit_log
 WHERE created_at >= '2026-05-16' AND created_at < '2026-05-17'
   AND org_id = 1
 ORDER BY created_at;
```

The audit-log viewer in the UI (Phase 6, admin-gated) does
free-text search across `diff_jsonb` plus the filterable columns —
SQL is only needed for queries the UI does not expose.

---

## Aircraft dedupe flow

This is the end-to-end shape of the dedupe story, tying together
identity, identifiers, counters, and audit.

### Auto-register from DJI log (PR #67 and earlier)

When a DJI log is imported and the log's reported aircraft SN does
not resolve via any of the three identifier-matching steps
(`serial_number` exact, `aircraft_identifiers` exact, prefix
tolerance), the flight-commit handler creates a new `aircraft` row
with:

- `serial_number = <log SN>` (which, for Matrice 600 / Inspire 1
  X5, is actually the FC SN)
- `needs_manual_sn = TRUE`
- `model_id` resolved (or created) via
  `services::aircraft_models::resolve_or_create` from the log's
  reported model name
- A best-effort `aircraft_identifiers` row of `source = fc_sn`
  (post-PR #67; previously was `body_sn`, which was the bug PR #67
  fixed) and a second row of `alias_djigo` or `alias_djifly` when
  the log carries a non-placeholder device alias.
- **No template-applied maintenance schedules.** Per the same PR,
  pending-review aircraft do not get default schedules seeded
  until their identity is confirmed (manual SN edit or merge).

### Discoverability in UI (PR #68)

The Fleet page displays a pending-review badge on each accordion
group that contains at least one `needs_manual_sn = TRUE` aircraft,
plus a "Show pending only" filter toggle. The operator can see at
a glance how many aircraft need review and jump straight to them.

### Resolution path A — manual SN edit

Operator opens the aircraft's EditAircraftModal, replaces the
auto-registered `serial_number` with the QR-label body SN, saves.
The `PUT /aircraft/:id` handler:

1. Updates `aircraft.serial_number`.
2. Auto-clears `needs_manual_sn`.
3. Writes an UPDATE audit row.

The previous (FC) SN remains on the aircraft as the `fc_sn`
identifier row — no data is lost.

### Resolution path B — merge into existing aircraft (PR #69 + #70)

When the operator recognises the pending-review aircraft as a
duplicate of one they already have, the EditAircraftModal exposes
a merge UI (PR #70). It calls `POST /api/v1/aircraft/:source_id/merge`
with `{ target_id }` (admin only). The handler, in a single
transaction:

1. Moves every `flights` row from source to target.
2. Moves the remote-controller binding (if any).
3. Moves `maintenance_schedules` rows.
4. Moves `aircraft_identifiers` rows, reclassifying the source's
   `body_sn` row as `fc_sn` on the target.
5. Recomputes `total_flights` / `total_seconds` / `last_flight` on
   the target.
6. Writes a DELETE audit row with `reason = "manual-merge"` and
   the operational metadata (see `diff_jsonb` shape above).
7. Deletes the source `aircraft` row.

### Resolution path C — manual SQL merge (legacy, pre-PR #69)

Before the merge endpoint existed, the only way to perform a
dedupe-merge was a pair of maintainer SQL scripts (a dry run and a
commit script; not published). The script does steps 1–7 from
the endpoint flow above, plus initial `\d audit_log` schema-print
diagnostics and post-merge verification SELECTs.

This path remains available as a backup if the endpoint is
unavailable or for unusual cases the endpoint does not cover (for
example, cross-org merges, which the endpoint refuses). For
ordinary operator-driven dedupe on the current production system,
prefer the endpoint — it is admin-audited, transactionally safe,
and reachable from the UI.

---

## Reference index

- `backend/migrations/` — Authoritative schema DDL
- [docs/log-parser-audit.md](./log-parser-audit.md) — Per-field parser
  coverage for binary, DJI Fly CSV, Litchi CSV
- [docs/roles-and-permissions.md](./roles-and-permissions.md) — Endpoint
  RBAC matrix
