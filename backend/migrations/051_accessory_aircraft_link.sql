-- 051_accessory_aircraft_link.sql
-- Adds the aircraft mount link deferred from migration 050.
-- Mirrors batteries / remote_controllers: a nullable direct FK, ON DELETE SET NULL.
ALTER TABLE accessories
    ADD COLUMN IF NOT EXISTS aircraft_id INTEGER REFERENCES aircraft(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_accessories_aircraft_id ON accessories(aircraft_id);
