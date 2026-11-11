-- Individual checklist items within a maintenance instruction template,
-- ordered by (section_order, item_order).
-- reference_manual may cite the source document and section,
-- e.g. 'DJI M600 Maintenance Manual V1.0 §I.1'.

CREATE TABLE maintenance_checklist_items (
    id               SERIAL PRIMARY KEY,
    template_id      INTEGER NOT NULL REFERENCES maintenance_instruction_templates(id) ON DELETE CASCADE,
    section_name     TEXT NOT NULL CHECK (length(trim(section_name)) > 0),
    section_order    INTEGER NOT NULL DEFAULT 0,
    item_order       INTEGER NOT NULL DEFAULT 0,
    item_text        TEXT NOT NULL CHECK (length(trim(item_text)) > 0),
    is_required      BOOLEAN NOT NULL DEFAULT TRUE,
    reference_manual TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mci_template_id ON maintenance_checklist_items (template_id);
CREATE INDEX idx_mci_section ON maintenance_checklist_items (template_id, section_order, item_order);

COMMENT ON TABLE maintenance_checklist_items IS
    'Individual checklist items within a maintenance instruction template, ordered by (section_order, item_order). reference_manual may cite the source document and section, e.g. DJI M600 Maintenance Manual V1.0 §I.1.';
