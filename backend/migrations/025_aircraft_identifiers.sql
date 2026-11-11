-- 025_aircraft_identifiers.sql
-- Adds aircraft_identifiers association table to allow one aircraft to carry
-- multiple identifiers from different sources. Required because DJI Inspire 1
-- (and some Matrice variants) report a flight controller serial number in the
-- log file that is structurally different from the body/printed serial number
-- the operator registered the aircraft under. The two SNs share zero common
-- prefix and cannot be reconciled by the 2-char tolerance in
-- sn_matching::find_by_serial.
--
-- aircraft.serial_number remains the canonical body SN. This table holds only
-- NON-canonical identifiers (FC SNs from logs, DJI GO/Fly aliases, manual
-- aliases, scanned QR labels). Lookup logic in sn_matching::find_match
-- consults aircraft.serial_number first, then this table, then falls back to
-- prefix-tolerance on aircraft.serial_number.

CREATE TABLE IF NOT EXISTS aircraft_identifiers (
    id            SERIAL PRIMARY KEY,
    aircraft_id   INTEGER NOT NULL REFERENCES aircraft(id) ON DELETE CASCADE,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL CHECK (length(trim(identifier)) > 0),
    source        TEXT NOT NULL CHECK (source IN (
                    'body_sn',         -- mirror of aircraft.serial_number (operator-registered)
                    'fc_sn',           -- flight controller SN from firmware/log
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

ALTER TABLE aircraft_identifiers ENABLE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS idx_aircraft_identifiers_aircraft_id
    ON aircraft_identifiers(aircraft_id);
CREATE INDEX IF NOT EXISTS idx_aircraft_identifiers_lookup
    ON aircraft_identifiers(org_id, identifier);
