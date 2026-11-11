-- Backfill equipment_id for rows that were auto-registered by the
-- flight-import path before the fix in
-- handlers/flights/commit.rs. Six rows in production at the time of
-- this migration: 2 aircraft, 1 battery, 3 controllers.
--
-- Each table is processed independently:
--   * LOCK TABLE … IN SHARE ROW EXCLUSIVE MODE prevents concurrent
--     INSERTs from running between the MAX read and our UPDATE; this
--     is the same race window that next_equipment_id() leaves open
--     in normal operation, but during a migration we want to close
--     it explicitly.
--   * regexp_replace strips non-digits from existing equipment_id
--     values so 'UAV-007' becomes '007' which casts to 7. NULLIF on
--     empty result protects against rows where the regex stripped
--     everything.
--   * ROW_NUMBER() OVER (ORDER BY id) gives the missing rows a stable
--     sequential offset added to MAX, so the assignment is
--     deterministic on rerun (idempotent because WHERE equipment_id
--     IS NULL drops out once filled).
--   * Retired rows are skipped — they will never appear in fleet
--     dropdowns and don't need an identifier.

BEGIN;

-- aircraft → UAV-NNN
LOCK TABLE aircraft IN SHARE ROW EXCLUSIVE MODE;

WITH max_existing AS (
    SELECT COALESCE(
        MAX(NULLIF(regexp_replace(equipment_id, '\D', '', 'g'), '')::int),
        0
    ) AS m
    FROM aircraft
    WHERE equipment_id IS NOT NULL
),
targets AS (
    SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn
    FROM aircraft
    WHERE equipment_id IS NULL
      AND status != 'retired'
)
UPDATE aircraft a
SET equipment_id = 'UAV-' || LPAD((max_existing.m + targets.rn)::text, 3, '0')
FROM targets, max_existing
WHERE a.id = targets.id;

-- batteries → BAT-NNN
LOCK TABLE batteries IN SHARE ROW EXCLUSIVE MODE;

WITH max_existing AS (
    SELECT COALESCE(
        MAX(NULLIF(regexp_replace(equipment_id, '\D', '', 'g'), '')::int),
        0
    ) AS m
    FROM batteries
    WHERE equipment_id IS NOT NULL
),
targets AS (
    SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn
    FROM batteries
    WHERE equipment_id IS NULL
      AND status != 'retired'
)
UPDATE batteries b
SET equipment_id = 'BAT-' || LPAD((max_existing.m + targets.rn)::text, 3, '0')
FROM targets, max_existing
WHERE b.id = targets.id;

-- remote_controllers → CTL-NNN
LOCK TABLE remote_controllers IN SHARE ROW EXCLUSIVE MODE;

WITH max_existing AS (
    SELECT COALESCE(
        MAX(NULLIF(regexp_replace(equipment_id, '\D', '', 'g'), '')::int),
        0
    ) AS m
    FROM remote_controllers
    WHERE equipment_id IS NOT NULL
),
targets AS (
    SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn
    FROM remote_controllers
    WHERE equipment_id IS NULL
      AND status != 'retired'
)
UPDATE remote_controllers c
SET equipment_id = 'CTL-' || LPAD((max_existing.m + targets.rn)::text, 3, '0')
FROM targets, max_existing
WHERE c.id = targets.id;

COMMIT;
