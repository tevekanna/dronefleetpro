-- Migration: data_transfer_natural_keys
--
-- Adds UNIQUE constraints on the natural keys used by the data
-- export/import feature for pilots and maintenance_schedules. These
-- are partial unique indexes to allow legitimate NULL/blank cases
-- (e.g. trainee pilots without a license).
--
-- License number is the EASA-issued pilot identifier; uniqueness per
-- org prevents duplicate pilot records on re-imports. Pilots without
-- a license_number fall back to email then name matching during import
-- (handled in code, no DB constraint).
--
-- Maintenance schedules are uniquely identified by (org, aircraft, name)
-- because the same aircraft can have many schedules but never two with
-- the same name (e.g. one "Annual service" per aircraft).

CREATE UNIQUE INDEX IF NOT EXISTS pilots_org_license_unique
    ON pilots(org_id, license_number)
    WHERE license_number IS NOT NULL AND license_number != '';

CREATE UNIQUE INDEX IF NOT EXISTS maint_schedules_org_aircraft_name_unique
    ON maintenance_schedules(org_id, aircraft_id, name);
