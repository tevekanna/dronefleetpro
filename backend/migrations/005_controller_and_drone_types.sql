-- 005_controller_and_drone_types.sql
-- Add controller tracking to flights and drone type learning system

-- Add controller_id to flights table
ALTER TABLE flights ADD COLUMN IF NOT EXISTS controller_id INTEGER REFERENCES remote_controllers(id);

CREATE INDEX IF NOT EXISTS idx_flights_controller_id ON flights(controller_id);

-- Drone type mapping table for learning system
-- When a user confirms an aircraft name during import, the mapping is stored
-- so future imports with the same drone_type_code are auto-resolved.
CREATE TABLE IF NOT EXISTS drone_type_mappings (
    id SERIAL PRIMARY KEY,
    org_id INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    app_source TEXT NOT NULL,
    drone_type_code TEXT NOT NULL,
    model_name TEXT NOT NULL,
    confirmed_by_user BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, app_source, drone_type_code)
);

CREATE INDEX IF NOT EXISTS idx_drone_type_mappings_lookup
    ON drone_type_mappings(org_id, app_source, drone_type_code);

ALTER TABLE drone_type_mappings ENABLE ROW LEVEL SECURITY;
