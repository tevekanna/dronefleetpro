-- 004_pilot_qualifications_update.sql
-- Add EASA SPEC qualification support to existing pilot_qualifications table.
-- The table already exists from 001_initial_schema with columns:
--   id, org_id, pilot_id, type_name, issued_date, expiry_date, certificate_ref, notes, created_at
-- We add a unique constraint on (pilot_id, type_name) to prevent duplicate qualifications.

CREATE UNIQUE INDEX IF NOT EXISTS idx_pilot_quals_unique
    ON pilot_qualifications(pilot_id, type_name);
