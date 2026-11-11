-- 052_battery_compatible_model_fk.sql
-- Structured battery<->model compatibility (PR A of the takeoff-mass arc).
-- The free-text compatible_model column is KEPT as log-derived source data;
-- this FK is the structured relation, opportunistically matched from it.
ALTER TABLE batteries
    ADD COLUMN IF NOT EXISTS compatible_model_id INTEGER REFERENCES aircraft_models(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_batteries_compatible_model_id ON batteries(compatible_model_id);

-- Backfill: exact case-insensitive name match within the same organisation.
-- Unmatched text stays text-only (compatible_model_id NULL) — never guess.
UPDATE batteries b
SET compatible_model_id = am.id
FROM aircraft_models am
WHERE b.compatible_model_id IS NULL
  AND b.compatible_model IS NOT NULL
  AND btrim(b.compatible_model) <> ''
  AND am.org_id = b.org_id
  AND LOWER(btrim(am.name)) = LOWER(btrim(b.compatible_model));
