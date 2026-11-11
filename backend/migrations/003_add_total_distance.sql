-- 003_add_total_distance.sql
-- Adds total_distance_m column to flights for actual cumulative distance flown.

ALTER TABLE flights ADD COLUMN IF NOT EXISTS total_distance_m REAL NOT NULL DEFAULT 0;
