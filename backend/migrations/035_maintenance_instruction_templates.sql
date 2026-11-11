-- Per-org maintenance instruction templates.
-- aircraft_model_id = NULL means the template is generic (not model-specific).
-- source='manufacturer' templates are seeded from official manuals;
-- source='custom' are user-created.
-- is_active=FALSE retires a template without deleting historical references.

CREATE TABLE maintenance_instruction_templates (
    id                  SERIAL PRIMARY KEY,
    org_id              INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    aircraft_model_id   INTEGER REFERENCES aircraft_models(id) ON DELETE SET NULL,
    name                TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description         TEXT,
    revision            TEXT NOT NULL DEFAULT '01',
    source              TEXT NOT NULL DEFAULT 'custom'
                            CHECK (source IN ('manufacturer', 'custom')),
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_by          INTEGER NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mit_org_id ON maintenance_instruction_templates (org_id);
CREATE INDEX idx_mit_model_id ON maintenance_instruction_templates (aircraft_model_id);

COMMENT ON TABLE maintenance_instruction_templates IS
    'Per-org maintenance instruction templates. aircraft_model_id = NULL means the template is generic (not model-specific). source=manufacturer templates are seeded from official manuals; source=custom are user-created. is_active=FALSE retires a template without deleting historical references.';
