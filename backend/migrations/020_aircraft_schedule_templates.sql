-- ============================================================================
-- Per-model maintenance schedule templates.
--
-- Templates define DEFAULT intervals for a model family. Schedules per
-- individual aircraft are still stored in maintenance_schedules and spawn
-- from these templates when an aircraft is registered.
--
-- Templates can be edited by admin; existing schedules are NOT auto-synced.
-- A separate "Sync from template" action updates affected aircraft schedules.
-- ============================================================================

CREATE TABLE maintenance_schedule_templates (
    id                       SERIAL PRIMARY KEY,
    org_id                   INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    model_id                 INTEGER NOT NULL REFERENCES aircraft_models(id) ON DELETE CASCADE,
    name                     TEXT NOT NULL,
    description              TEXT,
    interval_type            TEXT NOT NULL
        CHECK (interval_type IN ('flight_hours', 'calendar_days', 'cycles')),
    interval_value           REAL NOT NULL CHECK (interval_value > 0),
    warning_threshold        REAL NOT NULL DEFAULT 0.8
        CHECK (warning_threshold > 0 AND warning_threshold <= 1),
    is_critical              BOOLEAN NOT NULL DEFAULT FALSE,
    secondary_interval_type  TEXT
        CHECK (secondary_interval_type IS NULL
               OR secondary_interval_type IN ('flight_hours', 'calendar_days')),
    secondary_interval_value REAL
        CHECK (secondary_interval_value IS NULL OR secondary_interval_value > 0),
    is_active                BOOLEAN NOT NULL DEFAULT TRUE,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by               TEXT,
    CONSTRAINT mst_secondary_consistency CHECK (
        (secondary_interval_type IS NULL) = (secondary_interval_value IS NULL)
    ),
    UNIQUE (org_id, model_id, name)
);

CREATE INDEX idx_mst_org_model
    ON maintenance_schedule_templates(org_id, model_id)
    WHERE is_active = TRUE;

COMMENT ON TABLE maintenance_schedule_templates IS
    'Per-model default maintenance intervals. Spawns per-aircraft rows in maintenance_schedules at registration time.';
