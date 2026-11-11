-- Per-organisation branding: sidebar logo upload (light/dark variants).
-- One row per (org_id, variant). Logo stored as BYTEA so pg_dump backups
-- include it and there is no UPLOAD_DIR/disk dependency. The read endpoint is
-- open to every authenticated role (the sidebar renders for everyone); writes
-- are admin-only and audited.
CREATE TABLE IF NOT EXISTS organisation_branding (
    org_id            INTEGER     NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    variant           TEXT        NOT NULL CHECK (variant IN ('light', 'dark')),
    logo              BYTEA       NOT NULL,
    mime_type         TEXT        NOT NULL,
    size_bytes        INTEGER     NOT NULL,
    original_filename TEXT,
    updated_by        INTEGER,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, variant)
);
