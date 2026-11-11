-- 030_controller_and_battery_identifiers.sql
-- Adds controller_identifiers and battery_identifiers association tables so a
-- single remote controller or battery can carry multiple identifier strings
-- from different sources. Mirrors aircraft_identifiers (migration 025).
--
-- Background: dji-log-parser 0.5 returns FC-side / firmware-side serial
-- numbers for some DJI airframes (Inspire 1, M600, and other Matrice
-- variants) that are structurally different from the body / printed serial
-- number the operator registered the equipment under via QR scan. The 2-char
-- prefix tolerance in sn_matching::find_by_serial cannot bridge the gap
-- (the two SNs share zero common prefix). Aircraft already has this fix
-- (mig 025). Empirical evidence from production (an auto-registered controller under
-- its FC SN shadowing the real, QR-registered controller) shows controllers and batteries need
-- the same symmetric architecture.
--
-- remote_controllers.serial_number and batteries.serial_number remain the
-- canonical body SNs. These tables hold only NON-canonical identifiers
-- (FC SNs from logs, DJI GO/Fly aliases, manual aliases, scanned QR labels).
-- Lookup logic in sn_matching::find_match consults the equipment's
-- serial_number column first, then this table, then falls back to
-- prefix-tolerance on the serial_number column.
--
-- Also adds needs_manual_sn BOOLEAN to remote_controllers and batteries,
-- mirroring the aircraft.needs_manual_sn flag from migration 016, so the
-- DJI-log auto-register path in PR 2 can mark ghost rows for operator
-- review. Existing controllers with the auto-register signature (no model,
-- no nickname) are bulk-flagged here. Batteries are NOT bulk-flagged
-- because their ghost signature is too weak (compatible_model is often
-- blank for legitimate batteries pre-migration-011 normalisation).

-- ----------------------------------------------------------------------
-- controller_identifiers
-- ----------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS controller_identifiers (
    id            SERIAL PRIMARY KEY,
    controller_id INTEGER NOT NULL REFERENCES remote_controllers(id) ON DELETE CASCADE,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL CHECK (length(trim(identifier)) > 0),
    source        TEXT NOT NULL CHECK (source IN (
                    'body_sn',         -- mirror of remote_controllers.serial_number (operator-registered)
                    'fc_sn',           -- firmware/log-reported SN
                    'alias_djigo',     -- DJI GO alias seen in a log
                    'alias_djifly',    -- DJI Fly alias seen in a log
                    'alias_manual',    -- user-defined alias
                    'qr_label',        -- scanned QR label
                    'other'
                  )),
    observed_in   TEXT,                -- e.g. 'flight:1234' or 'manual'
    notes         TEXT,
    created_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, identifier, source)
);

ALTER TABLE controller_identifiers ENABLE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS idx_controller_identifiers_controller_id
    ON controller_identifiers(controller_id);
CREATE INDEX IF NOT EXISTS idx_controller_identifiers_lookup
    ON controller_identifiers(org_id, identifier);

-- ----------------------------------------------------------------------
-- battery_identifiers
-- ----------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS battery_identifiers (
    id            SERIAL PRIMARY KEY,
    battery_id    INTEGER NOT NULL REFERENCES batteries(id) ON DELETE CASCADE,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL CHECK (length(trim(identifier)) > 0),
    source        TEXT NOT NULL CHECK (source IN (
                    'body_sn',         -- mirror of batteries.serial_number (operator-registered)
                    'fc_sn',           -- firmware/log-reported SN
                    'alias_djigo',     -- DJI GO alias seen in a log
                    'alias_djifly',    -- DJI Fly alias seen in a log
                    'alias_manual',    -- user-defined alias
                    'qr_label',        -- scanned QR label
                    'other'
                  )),
    observed_in   TEXT,                -- e.g. 'flight:1234' or 'manual'
    notes         TEXT,
    created_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, identifier, source)
);

ALTER TABLE battery_identifiers ENABLE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS idx_battery_identifiers_battery_id
    ON battery_identifiers(battery_id);
CREATE INDEX IF NOT EXISTS idx_battery_identifiers_lookup
    ON battery_identifiers(org_id, identifier);

-- ----------------------------------------------------------------------
-- needs_manual_sn columns + partial indices (mirror of mig 016 for aircraft)
-- ----------------------------------------------------------------------
ALTER TABLE remote_controllers
    ADD COLUMN IF NOT EXISTS needs_manual_sn BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE batteries
    ADD COLUMN IF NOT EXISTS needs_manual_sn BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_remote_controllers_needs_manual_sn
    ON remote_controllers(org_id)
    WHERE needs_manual_sn = TRUE;

CREATE INDEX IF NOT EXISTS idx_batteries_needs_manual_sn
    ON batteries(org_id)
    WHERE needs_manual_sn = TRUE;

COMMENT ON COLUMN remote_controllers.needs_manual_sn IS
  'TRUE for controllers auto-registered from a DJI log SN that did not match an existing row. The log SN may be the FC SN, not the chassis body SN, so the row may be a duplicate. Operator confirms or merges via the pending-review UI.';
COMMENT ON COLUMN batteries.needs_manual_sn IS
  'TRUE for batteries auto-registered from a DJI log SN that did not match an existing row. The log SN may be the FC SN, not the chassis body SN, so the row may be a duplicate. Operator confirms or merges via the pending-review UI.';

-- ----------------------------------------------------------------------
-- Backfill body_sn identifiers from existing canonical SNs.
-- ON CONFLICT DO NOTHING keeps the migration idempotent if a body_sn row
-- already exists (e.g. from a future re-run).
-- ----------------------------------------------------------------------
INSERT INTO controller_identifiers (controller_id, org_id, identifier, source, observed_in)
SELECT id, org_id, serial_number, 'body_sn', 'migration_030_backfill'
  FROM remote_controllers
 WHERE serial_number IS NOT NULL
   AND length(trim(serial_number)) > 0
ON CONFLICT (org_id, identifier, source) DO NOTHING;

INSERT INTO battery_identifiers (battery_id, org_id, identifier, source, observed_in)
SELECT id, org_id, serial_number, 'body_sn', 'migration_030_backfill'
  FROM batteries
 WHERE serial_number IS NOT NULL
   AND length(trim(serial_number)) > 0
ON CONFLICT (org_id, identifier, source) DO NOTHING;

-- ----------------------------------------------------------------------
-- Flag existing auto-register-signature controllers for operator review.
-- The DJI-log auto-register path created these rows with no model and no
-- nickname (only serial_number + equipment_id). Real QR-imported controllers
-- always carry at least one of model / nickname / compatible_models.
-- Batteries are NOT bulk-flagged here — see header comment for rationale.
-- ----------------------------------------------------------------------
UPDATE remote_controllers
   SET needs_manual_sn = TRUE
 WHERE (model IS NULL OR length(trim(model)) = 0)
   AND (nickname IS NULL OR length(trim(nickname)) = 0);
