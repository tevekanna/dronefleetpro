-- Migration: data_export_import support
--
-- Adds a tracking table for export/import operations. The audit_log
-- table already accepts arbitrary table_name values, so no constraint
-- changes are needed there — export/import operations will write rows
-- with table_name = 'data_export' or 'data_import'.

CREATE TABLE IF NOT EXISTS data_transfer_jobs (
    id              BIGSERIAL PRIMARY KEY,
    org_id          INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    user_id         INTEGER NOT NULL REFERENCES users(id),
    user_code       TEXT NOT NULL,
    job_type        TEXT NOT NULL CHECK (job_type IN ('export', 'import')),
    status          TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    entity_groups   JSONB NOT NULL,                -- ['equipment', 'pilots', ...]
    options         JSONB,                          -- strategy, dry_run, scrub_secrets, etc.
    summary         JSONB,                          -- counts per entity, conflicts, errors
    error_message   TEXT,
    started_at      TIMESTAMPTZ DEFAULT NOW(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_data_transfer_jobs_org ON data_transfer_jobs(org_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_data_transfer_jobs_user ON data_transfer_jobs(user_id);

ALTER TABLE data_transfer_jobs ENABLE ROW LEVEL SECURITY;

CREATE POLICY data_transfer_jobs_org_isolation ON data_transfer_jobs
    USING (org_id = current_setting('app.current_org_id', TRUE)::INTEGER);
