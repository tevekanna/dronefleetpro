-- Contract half of expand-contract for flight_id FK.
-- Removes legacy free-text flight_log_ref column from maintenance and defect_log.
-- Historical values are preserved in audit_log.diff_jsonb (INSERT entries).

ALTER TABLE maintenance DROP COLUMN IF EXISTS flight_log_ref;
ALTER TABLE defect_log DROP COLUMN IF EXISTS flight_log_ref;
