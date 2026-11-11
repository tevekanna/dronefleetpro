-- Airworthiness status is managed independently of operational status
-- (active/retired). The Maintenance Manager sets this to reflect whether
-- the aircraft is cleared for flight from a maintenance perspective.
--
-- Values:
--   airworthy     - cleared for flight (default for existing rows)
--   not_airworthy - grounded by maintenance decision
--
-- Rationale: 'status' = active/retired is an operational/lifecycle flag.
-- 'airworthiness_status' is a maintenance/airworthiness flag. They are
-- orthogonal: an active aircraft can be not_airworthy (awaiting parts),
-- and a retired aircraft is always implicitly not_airworthy.
--
-- All existing aircraft rows default to 'airworthy' on migration.
-- New aircraft created via POST /api/v1/aircraft will default to
-- 'not_airworthy' (enforced in the handler, not the column default)
-- so the operator must explicitly confirm airworthiness before first flight.

ALTER TABLE aircraft
    ADD COLUMN IF NOT EXISTS airworthiness_status TEXT NOT NULL
        DEFAULT 'airworthy'
        CHECK (airworthiness_status IN ('airworthy', 'not_airworthy'));

-- Optional: index for hub queries that filter by airworthiness_status
CREATE INDEX IF NOT EXISTS idx_aircraft_airworthiness_status
    ON aircraft (org_id, airworthiness_status)
    WHERE status = 'active';

COMMENT ON COLUMN aircraft.airworthiness_status IS
    'Maintenance Manager-controlled airworthiness flag. airworthy = cleared for flight. not_airworthy = grounded pending maintenance. Independent of operational status (active/retired).';
