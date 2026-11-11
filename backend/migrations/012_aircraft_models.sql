-- ============================================================
-- Aircraft models as a first-class entity (per-org catalog).
--
-- Before this migration, `aircraft.model` was free-text and
-- `drone_type_mappings.model_name` referenced model names as strings.
-- After this migration, both tables point at `aircraft_models.id`.
--
-- This migration is one-shot (it drops `aircraft.model` at the end);
-- re-running it against a database where the column is already gone
-- will fail loudly on Step 6 — intended.
-- ============================================================

-- ---------------- Step 1: Catalog table ----------------------
CREATE TABLE IF NOT EXISTS aircraft_models (
    id                              SERIAL PRIMARY KEY,
    org_id                          INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    name                            TEXT NOT NULL,
    manufacturer                    TEXT,
    -- `category` is free text for now; convention is "consumer" /
    -- "enterprise" / "custom". A CHECK constraint will be added if
    -- the vocabulary stabilises.
    category                        TEXT,
    is_active                       BOOLEAN NOT NULL DEFAULT TRUE,
    notes                           TEXT,
    default_max_flight_time_minutes INTEGER,
    default_weight_kg               REAL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Case-insensitive uniqueness per org so "DJI Air 2s" and "DJI AIR 2S"
-- are treated as the same model in a single organisation.
CREATE UNIQUE INDEX IF NOT EXISTS aircraft_models_org_lower_name_key
    ON aircraft_models (org_id, LOWER(name));

CREATE INDEX IF NOT EXISTS idx_aircraft_models_org_active
    ON aircraft_models (org_id, is_active);

-- ---------------- Step 2: aircraft.model_id ------------------
ALTER TABLE aircraft ADD COLUMN IF NOT EXISTS model_id INTEGER
    REFERENCES aircraft_models(id) ON DELETE SET NULL;

-- ---------------- Step 3: Backfill from aircraft.model -------
-- DISTINCT ON picks the lowest-id aircraft per (org, lowercased name)
-- and uses THAT row's casing as the canonical `aircraft_models.name`.
INSERT INTO aircraft_models (org_id, name)
SELECT DISTINCT ON (org_id, LOWER(TRIM(model)))
    org_id,
    TRIM(model)
FROM aircraft
WHERE model IS NOT NULL AND TRIM(model) <> ''
ORDER BY org_id, LOWER(TRIM(model)), id
ON CONFLICT (org_id, LOWER(name)) DO NOTHING;

-- ---------------- Step 4: Link aircraft rows -----------------
UPDATE aircraft a
SET model_id = am.id
FROM aircraft_models am
WHERE a.org_id = am.org_id
  AND a.model IS NOT NULL
  AND LOWER(TRIM(a.model)) = LOWER(am.name);

-- ---------------- Step 5: drone_type_mappings ----------------
-- Add the FK column, backfill from existing model_name strings,
-- fill in any misses by creating a new aircraft_models row, then
-- tighten to NOT NULL and drop model_name.

ALTER TABLE drone_type_mappings ADD COLUMN IF NOT EXISTS model_id INTEGER
    REFERENCES aircraft_models(id) ON DELETE CASCADE;

UPDATE drone_type_mappings dtm
SET model_id = am.id
FROM aircraft_models am
WHERE dtm.org_id = am.org_id
  AND LOWER(TRIM(dtm.model_name)) = LOWER(am.name);

-- For mappings whose model_name never appeared in the aircraft
-- table (e.g. the mapping was learned for a drone the org doesn't
-- own), create a model row per distinct remaining string.
INSERT INTO aircraft_models (org_id, name)
SELECT DISTINCT ON (dtm.org_id, LOWER(TRIM(dtm.model_name)))
    dtm.org_id,
    TRIM(dtm.model_name)
FROM drone_type_mappings dtm
WHERE dtm.model_id IS NULL
  AND dtm.model_name IS NOT NULL
  AND TRIM(dtm.model_name) <> ''
ORDER BY dtm.org_id, LOWER(TRIM(dtm.model_name)), dtm.id
ON CONFLICT (org_id, LOWER(name)) DO NOTHING;

-- Re-run the backfill to pick up the newly-created rows.
UPDATE drone_type_mappings dtm
SET model_id = am.id
FROM aircraft_models am
WHERE dtm.model_id IS NULL
  AND dtm.org_id = am.org_id
  AND dtm.model_name IS NOT NULL
  AND LOWER(TRIM(dtm.model_name)) = LOWER(am.name);

ALTER TABLE drone_type_mappings ALTER COLUMN model_id SET NOT NULL;
ALTER TABLE drone_type_mappings DROP COLUMN model_name;

-- ---------------- Step 6: Drop aircraft.model ----------------
ALTER TABLE aircraft DROP COLUMN model;

-- ---------------- Verification (RAISE NOTICE) ----------------
DO $$
DECLARE
    model_count       INTEGER;
    aircraft_linked   INTEGER;
    aircraft_unlinked INTEGER;
    mapping_count     INTEGER;
    per_org           RECORD;
BEGIN
    SELECT COUNT(*) INTO model_count FROM aircraft_models;
    SELECT COUNT(*) INTO aircraft_linked   FROM aircraft WHERE model_id IS NOT NULL;
    SELECT COUNT(*) INTO aircraft_unlinked FROM aircraft WHERE model_id IS NULL;
    SELECT COUNT(*) INTO mapping_count     FROM drone_type_mappings;

    RAISE NOTICE '[012] aircraft_models rows created: %', model_count;
    RAISE NOTICE '[012] aircraft rows linked to a model: %', aircraft_linked;
    RAISE NOTICE '[012] aircraft rows with NULL model_id (were NULL/empty pre-migration): %', aircraft_unlinked;
    RAISE NOTICE '[012] drone_type_mappings rows migrated (all have model_id NOT NULL): %', mapping_count;

    FOR per_org IN
        SELECT org_id, COUNT(*) AS n FROM aircraft_models GROUP BY org_id ORDER BY org_id
    LOOP
        RAISE NOTICE '[012]   org_id=%: % model(s)', per_org.org_id, per_org.n;
    END LOOP;
END $$;
