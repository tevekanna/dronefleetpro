-- Replace the legacy v1-era checklist stubs (created in migration 001 but
-- never wired to any endpoint or UI; all three tables verified empty across
-- the entire DB during Phase 0 of PR #4). The old shape stored `model` as
-- free text, allowed only 'normal'/'qrh', had no PF/PM/N/A responsibility
-- column, and had no page-break flag. We DROP CASCADE and recreate.
--
-- New shape:
--   - checklists: per-model FK to aircraft_models, type ('normal_ops' /
--     'qrh'), revision metadata, audit FK to users for created_by /
--     updated_by, UNIQUE (model_id, type) so the catalog holds at most one
--     of each type per model.
--   - checklist_sections: title + sort_order + start_on_new_page flag (used
--     by the PDF export in a downstream PR).
--   - checklist_items: label + free-text value + responsibility enum
--     ('PF' / 'PM' / 'PF/PM' / 'N/A') + sort_order.
--
-- Cascade semantics: deleting a model deletes its checklists; deleting a
-- checklist deletes its sections; deleting a section deletes its items.

DROP TABLE IF EXISTS checklist_items CASCADE;
DROP TABLE IF EXISTS checklist_sections CASCADE;
DROP TABLE IF EXISTS checklists CASCADE;

CREATE TABLE checklists (
    id              SERIAL PRIMARY KEY,
    org_id          INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    model_id        INTEGER NOT NULL REFERENCES aircraft_models(id) ON DELETE CASCADE,
    type            TEXT NOT NULL CHECK (type IN ('normal_ops', 'qrh')),
    title           TEXT NOT NULL CHECK (length(trim(title)) > 0),
    revision        TEXT NOT NULL DEFAULT '01',
    revision_date   DATE,
    created_by      INTEGER NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    updated_by      INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (model_id, type)
);

CREATE INDEX idx_checklists_org_id ON checklists (org_id);
CREATE INDEX idx_checklists_model_id ON checklists (model_id);

CREATE TABLE checklist_sections (
    id                  SERIAL PRIMARY KEY,
    checklist_id        INTEGER NOT NULL REFERENCES checklists(id) ON DELETE CASCADE,
    title               TEXT NOT NULL CHECK (length(trim(title)) > 0),
    sort_order          INTEGER NOT NULL DEFAULT 0,
    start_on_new_page   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_checklist_sections_checklist ON checklist_sections (checklist_id);

CREATE TABLE checklist_items (
    id              SERIAL PRIMARY KEY,
    section_id      INTEGER NOT NULL REFERENCES checklist_sections(id) ON DELETE CASCADE,
    label           TEXT NOT NULL CHECK (length(trim(label)) > 0),
    value           TEXT NOT NULL,
    responsibility  TEXT NOT NULL CHECK (responsibility IN ('PF', 'PM', 'PF/PM', 'N/A')),
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_checklist_items_section ON checklist_items (section_id);

COMMENT ON TABLE checklists IS
    'Per-model operating checklists (Normal Ops and QRH). One checklist per (model_id, type). Editable by admin; viewable by all roles. Replaces the v1-era legacy stubs.';
