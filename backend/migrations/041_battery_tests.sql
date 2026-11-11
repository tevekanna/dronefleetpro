-- 041_battery_tests.sql
-- Battery test log (EASA SORA OSO#3: operators must evidence periodic
-- battery condition checks). The battery_tests table already exists as a
-- legacy stub from 001_initial_schema.sql; this migration extends it in
-- place rather than recreating it, so existing code paths (data_import,
-- export, purge) keep working unchanged.
--
-- Column conventions on the reused legacy columns:
--   capacity_measured    — measured capacity in Wh
--   internal_resistance  — internal resistance in mΩ (milliohms)
--
-- No side effects on the batteries table — this is a pure record.
ALTER TABLE battery_tests ADD COLUMN IF NOT EXISTS swelling_status TEXT NOT NULL DEFAULT 'none'
    CHECK (swelling_status IN ('none','minor','severe'));
ALTER TABLE battery_tests ADD COLUMN IF NOT EXISTS cell_voltages JSONB;
CREATE INDEX IF NOT EXISTS idx_battery_tests_battery ON battery_tests(battery_id, test_date DESC);
