-- Migration 027: Add flight_id FK to maintenance and defect_log.
-- Mirrors incidents.flight_id (001:244-248) — INTEGER REFERENCES flights(id)
-- ON DELETE SET NULL, plus a plain (flight_id) index per table.
--
-- This is the "expand" half of an expand-contract migration. The legacy
-- flight_log_ref TEXT column is left in place; UI cutover and DROP COLUMN
-- happen in a follow-up migration.

ALTER TABLE maintenance
    ADD COLUMN IF NOT EXISTS flight_id INTEGER REFERENCES flights(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_maintenance_flight_id ON maintenance(flight_id);

ALTER TABLE defect_log
    ADD COLUMN IF NOT EXISTS flight_id INTEGER REFERENCES flights(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_defect_log_flight_id ON defect_log(flight_id);
