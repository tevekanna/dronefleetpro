CREATE TABLE maintenance_drafts (
  id              SERIAL PRIMARY KEY,
  org_id          INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
  schedule_id     INTEGER NOT NULL REFERENCES maintenance_schedules(id) ON DELETE CASCADE,
  saved_by        TEXT NOT NULL,
  form_data       JSONB NOT NULL DEFAULT '{}',
  checklist_status JSONB NOT NULL DEFAULT '{}',
  checklist_notes  JSONB NOT NULL DEFAULT '{}',
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (org_id, schedule_id)
);
CREATE INDEX idx_maintenance_drafts_org ON maintenance_drafts(org_id);
