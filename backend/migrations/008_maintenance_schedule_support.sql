-- Maintenance schedule support:
--   * Link completed maintenance rows to the schedule that triggered them (EASA 4.2 traceability)
--   * Allow a schedule to track a secondary trigger (e.g. annual OR every 50 flight hours)
--   * Record who created the schedule

ALTER TABLE maintenance
    ADD COLUMN IF NOT EXISTS schedule_id INTEGER REFERENCES maintenance_schedules(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_maintenance_schedule_id ON maintenance(schedule_id);

ALTER TABLE maintenance_schedules
    ADD COLUMN IF NOT EXISTS secondary_interval_type  TEXT,
    ADD COLUMN IF NOT EXISTS secondary_interval_value REAL,
    ADD COLUMN IF NOT EXISTS secondary_next_due_at    TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS secondary_next_due_hours REAL,
    ADD COLUMN IF NOT EXISTS created_by               TEXT;

-- created_at already exists with NOT NULL DEFAULT NOW() in 001_initial_schema.sql.

CREATE INDEX IF NOT EXISTS idx_maint_sched_aircraft_active
    ON maintenance_schedules(aircraft_id) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_maint_sched_next_due_active
    ON maintenance_schedules(next_due_at) WHERE is_active = TRUE;

-- RLS already enabled on maintenance_schedules in 001_initial_schema.sql.
