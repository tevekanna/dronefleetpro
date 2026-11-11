-- 050_accessories_and_battery_weight.sql
-- Two changes, both idempotent and append-only.
--
-- (a) `accessories` — a weighted equipment inventory: any piece of fitted
--     equipment that carries a mass (camera, gimbal, payload module, mount,
--     sensor, ...). Org-scoped and shaped like batteries / remote_controllers,
--     but with NO aircraft linkage in this migration. The aircraft<->accessory
--     relationship and the take-off-mass calculation are deliberately deferred
--     to a later PR; this migration only establishes the inventory + weight.
--
-- (b) `batteries.weight_kg` — measured per-unit battery mass in kg.
--     WHY this is needed: the existing `batteries.capacity_wh` column is
--     ENERGY (watt-hours), not MASS. The planned take-off-mass vs MTOM check
--     (groundwork laid in migration 049) is
--         take-off mass = airframe empty weight + battery weight + payload
--     and the battery is typically the single largest removable mass on a DJI
--     airframe. Without a battery mass column that chain has no battery input
--     regardless of accessories. This adds the missing input; the calculation
--     itself remains a later-PR concern. migration 049 fields are untouched.

-- ----------------------------------------------------------------------
-- (a) accessories
-- ----------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS accessories (
    id               SERIAL PRIMARY KEY,
    org_id           INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    category         TEXT,               -- free text (e.g. camera, gimbal, payload, mount, sensor)
    manufacturer     TEXT,
    serial_number    TEXT,               -- NULLABLE: many accessories carry no SN
    equipment_id     TEXT,               -- human code, mirrors how batteries handle equipment_id
    weight_kg        REAL,               -- measured per-unit mass in kg
    status           TEXT NOT NULL DEFAULT 'active',
    retirement_notes TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partial unique index: multiple no-SN accessories are allowed (SN is NULL),
-- but a present SN stays unique per org.
CREATE UNIQUE INDEX IF NOT EXISTS accessories_org_serial_key
    ON accessories (org_id, serial_number) WHERE serial_number IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_accessories_org_status
    ON accessories (org_id, status);

-- ----------------------------------------------------------------------
-- (b) battery mass (energy vs mass; take-off-mass chain — see header)
-- ----------------------------------------------------------------------
ALTER TABLE batteries
    ADD COLUMN IF NOT EXISTS weight_kg REAL;   -- measured per-unit mass in kg
