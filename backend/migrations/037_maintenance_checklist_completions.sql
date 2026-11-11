-- Add nullable FK from maintenance records to instruction templates.
ALTER TABLE maintenance
    ADD COLUMN IF NOT EXISTS instruction_template_id
        INTEGER REFERENCES maintenance_instruction_templates(id) ON DELETE SET NULL;

-- Per-maintenance-record checklist completion state.
-- One row per (maintenance_id, checklist_item_id).
-- status='nok' should trigger a defect_log entry in the application layer.
-- completed_by stores the 3-letter user_code of the technician who marked the item.
-- The UNIQUE constraint prevents duplicate completion rows for the same item
-- within a maintenance record.

CREATE TABLE maintenance_checklist_completions (
    id                  SERIAL PRIMARY KEY,
    maintenance_id      INTEGER NOT NULL REFERENCES maintenance(id) ON DELETE CASCADE,
    checklist_item_id   INTEGER NOT NULL REFERENCES maintenance_checklist_items(id) ON DELETE RESTRICT,
    status              TEXT NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('ok', 'nok', 'na', 'pending')),
    note                TEXT,
    completed_by        TEXT,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (maintenance_id, checklist_item_id)
);

CREATE INDEX idx_mcc_maintenance_id ON maintenance_checklist_completions (maintenance_id);
CREATE INDEX idx_mcc_item_id ON maintenance_checklist_completions (checklist_item_id);

COMMENT ON TABLE maintenance_checklist_completions IS
    'Per-maintenance-record checklist completion state. One row per (maintenance_id, checklist_item_id). status=nok should trigger a defect_log entry in the application layer. completed_by stores the 3-letter user_code of the technician who marked the item.';
