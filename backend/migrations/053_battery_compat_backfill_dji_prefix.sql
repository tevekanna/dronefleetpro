-- 053_battery_compat_backfill_dji_prefix.sql
-- Second-pass backfill for migration 052: strip the "DJI " vendor prefix
-- from the log-derived compatible_model text, then exact case-insensitive
-- name match within the organisation. Still deterministic: the unique
-- index on (org_id, LOWER(name)) guarantees at most one match per org.
-- Labels that match no catalogue name after stripping stay NULL.
-- The compatible_model text itself is NOT modified (log-derived source data).
UPDATE batteries b
SET compatible_model_id = am.id
FROM aircraft_models am
WHERE b.compatible_model_id IS NULL
  AND b.compatible_model IS NOT NULL
  AND btrim(b.compatible_model) <> ''
  AND am.org_id = b.org_id
  AND LOWER(btrim(am.name)) = LOWER(btrim(regexp_replace(b.compatible_model, '^\s*[Dd][Jj][Ii]\s+', '')));
