-- Scope flight_guid uniqueness to non-deleted flights.
--
-- flights.flight_guid was created with a plain, table-wide UNIQUE constraint
-- (flights_flight_guid_key), so a soft-deleted flight still reserved its guid
-- for the whole table. Both the ArduPilot import dedup (handlers/flights/
-- import.rs) and the commit merge lookup (handlers/flights/commit.rs) filter on
-- `deleted_at IS NULL`, so a soft-deleted row was invisible to those checks yet
-- still blocked a fresh INSERT with the same guid — surfacing as a 23505
-- duplicate-key 500 when re-importing the same log after a delete (e.g.
-- re-importing a .bin following a test delete).
--
-- Replacing the global constraint with a partial unique index scoped to
-- `deleted_at IS NULL` makes guid uniqueness apply only among live flights,
-- exactly matching the deleted_at IS NULL filter those lookups already use, so
-- deleting a flight and re-importing the same log is permitted.
--
-- The existing non-unique idx_flights_guid (partial on flight_guid IS NOT NULL)
-- is left untouched.

ALTER TABLE flights DROP CONSTRAINT IF EXISTS flights_flight_guid_key;

CREATE UNIQUE INDEX IF NOT EXISTS flights_flight_guid_active_key
    ON flights (flight_guid)
    WHERE deleted_at IS NULL AND flight_guid IS NOT NULL;

COMMENT ON INDEX flights_flight_guid_active_key IS
    'flight_guid must be unique only among non-deleted flights (deleted_at IS NULL). '
    'This lets a flight be soft-deleted and the same log re-imported (e.g. re-importing '
    'a .bin after a test delete), consistent with the deleted_at IS NULL filter used by '
    'the import dedup and the commit merge lookup.';
