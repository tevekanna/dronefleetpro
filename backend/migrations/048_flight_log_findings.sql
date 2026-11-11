-- 048_flight_log_findings.sql
-- Operationally-relevant findings extracted from flight logs (PR: Feature 2).
--
-- Scope today is ArduPilot .bin (DataFlash) only — ERR (subsystem/error-code),
-- a curated subset of EV events, and an aggregated VIBE (vibration/clipping)
-- finding. The table is deliberately SOURCE-AGNOSTIC (`source` column) so DJI
-- extraction can be added later without a schema change; `subsys_id`/`ecode`
-- are ArduPilot-specific and nullable (NULL for EV/VIBE findings and for any
-- future non-ArduPilot source).
--
-- Findings are children of a flight, one row per distinct `code`. Aggregation
-- (occurrence_count, first/last offset, resolved) happens in the parser during
-- a single streaming pass; the table stores the already-deduplicated result.
--
-- RLS: matches the sibling child tables of flights/equipment (aircraft_
-- identifiers, controller_identifiers, battery_identifiers) — RLS is ENABLED
-- as an org-isolation safety net; no explicit policy is attached (the app
-- connects as the table owner and scopes every query by org_id in code). See
-- the internal design notes §7 "Multi-tenancy isolation".

CREATE TABLE IF NOT EXISTS flight_log_findings (
    id                   SERIAL PRIMARY KEY,
    org_id               INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    flight_id            INTEGER NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
    source               TEXT NOT NULL,                 -- 'ardupilot_bin' (only source today)
    code                 TEXT NOT NULL,                 -- stable slug, e.g. 'err.failsafe_batt.1', 'ev.ekf_yaw_reset', 'vibe.high'
    subsystem            TEXT,                          -- human subsystem name, nullable
    subsys_id            INTEGER,                       -- raw ArduPilot LogErrorSubsystem value, nullable
    ecode                INTEGER,                       -- raw ArduPilot LogErrorCode value, nullable (NULL for EV/VIBE)
    severity             TEXT NOT NULL CHECK (severity IN ('low','medium','high','critical')),
    message              TEXT NOT NULL,
    occurrence_count     INTEGER NOT NULL DEFAULT 1,
    first_offset_seconds REAL,                          -- seconds relative to segment arm (may be negative for pre-arm/ground events)
    last_offset_seconds  REAL,
    resolved             BOOLEAN NOT NULL DEFAULT false, -- an ECode=0 clear for the same subsystem was seen
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (flight_id, code)
);

ALTER TABLE flight_log_findings ENABLE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS idx_flight_log_findings_flight_id ON flight_log_findings(flight_id);
CREATE INDEX IF NOT EXISTS idx_flight_log_findings_org_id ON flight_log_findings(org_id);

COMMENT ON TABLE flight_log_findings IS
    'Operationally-relevant findings extracted from flight logs (ArduPilot .bin today; source-agnostic for future DJI support). One row per distinct code per flight; aggregation done in the parser. subsys_id/ecode are raw ArduPilot LogErrorSubsystem/LogErrorCode values (nullable). first/last_offset_seconds are relative to segment arm and may be negative for pre-arm ground events.';
