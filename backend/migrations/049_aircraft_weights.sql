-- 049_aircraft_weights.sql
-- Adds regulatory weight and EASA class-marking data to the aircraft_models
-- catalog, plus a measured empty weight on the individual airframe.
--
-- Why these fields (regulatory rationale, not decoration):
--   * mtom_kg (maximum take-off mass) is the regulatory ceiling. It
--     determines the Open-category subcategory (A1/A2/A3) an aircraft may
--     operate in and interacts with the EASA class marking (C0..C4). It is
--     the value a later take-off-mass check will be measured against.
--   * empty_weight_kg / max_payload_kg let take-off mass be derived
--     (empty airframe + battery + payload) and later checked against
--     mtom_kg. The take-off-mass vs MTOM guard is a planned follow-up.
--   * class_marking records the EASA class marking (C0..C6) where the
--     airframe carries one. Many older airframes carry no class marking at
--     all, so it is nullable and its absence is meaningful ("not marked"),
--     never missing data.
--   * max_dimension_cm (maximum characteristic dimension) is a direct input
--     to SORA ground-risk assessment.
--
-- default_weight_kg (migration 012) is intentionally LEFT UNTOUCHED. It is a
-- semantically-vague legacy field that stays in the export/import canonical
-- set (renaming or dropping it would break data transfer); the new fields
-- above supersede it for all regulatory purposes.

-- Model-level (catalogue / nominal) regulatory figures.
ALTER TABLE aircraft_models
    ADD COLUMN IF NOT EXISTS mtom_kg          REAL,   -- maximum take-off mass; the regulatory ceiling
    ADD COLUMN IF NOT EXISTS empty_weight_kg  REAL,   -- airframe without battery or payload
    ADD COLUMN IF NOT EXISTS max_payload_kg   REAL,
    ADD COLUMN IF NOT EXISTS class_marking    TEXT,   -- EASA class marking, nullable
    ADD COLUMN IF NOT EXISTS max_dimension_cm REAL;   -- maximum characteristic dimension

-- EASA class marking is a small closed enum where present; NULL means the
-- airframe carries no class marking at all. Guarded in a DO block so the
-- migration stays idempotent (ADD CONSTRAINT has no IF NOT EXISTS form).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'aircraft_models_class_marking_check'
    ) THEN
        ALTER TABLE aircraft_models
            ADD CONSTRAINT aircraft_models_class_marking_check
            CHECK (class_marking IS NULL
                   OR class_marking IN ('C0','C1','C2','C3','C4','C5','C6'));
    END IF;
END $$;

-- Measured empty weight of THIS individual airframe in its current
-- configuration (airframe without battery or payload). Distinct from the
-- model-level empty_weight_kg, which is the catalogue/nominal figure: a
-- physical airframe may weigh more than its model nominal once modifications,
-- ballast, or mounts are fitted.
ALTER TABLE aircraft
    ADD COLUMN IF NOT EXISTS empty_weight_kg REAL;
