-- Incident attachments + append-only incident notes.
--
-- incident_attachments: binary evidence for an occurrence (ArduPilot .bin/.tlog
-- logs, DJI logs, PDFs, images, etc.). Content lives on the filesystem under
-- ${UPLOAD_DIR}/incidents/{incident_id}/{attachment_id}.{ext}, mirroring the
-- model_documents storage convention (row keyed 1:1 to an on-disk file, so
-- deletes leave a clean correspondence and the audit log captures the original
-- filename + size at upload time).
--
-- incident_notes: a running, append-only commentary on an occurrence. There is
-- deliberately no update or delete in the API — the original occurrence record
-- must remain intact for EASA traceability.

CREATE TABLE IF NOT EXISTS incident_attachments (
    id                SERIAL PRIMARY KEY,
    incident_id       INTEGER NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    org_id            INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    kind              TEXT NOT NULL DEFAULT 'other'
                          CHECK (kind IN ('ardupilot_bin', 'ardupilot_tlog', 'log', 'document', 'image', 'other')),
    original_filename TEXT NOT NULL,
    file_path         TEXT NOT NULL,
    size_bytes        BIGINT NOT NULL CHECK (size_bytes > 0),
    mime_type         TEXT NOT NULL,
    uploaded_by       INTEGER NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    uploaded_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_incident_attachments_incident_id ON incident_attachments(incident_id);
CREATE INDEX IF NOT EXISTS idx_incident_attachments_org_id ON incident_attachments(org_id);

CREATE TABLE IF NOT EXISTS incident_notes (
    id          SERIAL PRIMARY KEY,
    incident_id INTEGER NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    org_id      INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    note        TEXT NOT NULL CHECK (length(trim(note)) > 0),
    created_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_incident_notes_incident_id ON incident_notes(incident_id);

COMMENT ON TABLE incident_attachments IS
    'Binary evidence attached to an incident (ArduPilot .bin/.tlog logs, DJI logs, PDFs, images, and other reference files). File content lives on the filesystem under ${UPLOAD_DIR}/incidents/{incident_id}/{attachment_id}.{ext}, mirroring model_documents; the row is keyed 1:1 to its on-disk file.';

COMMENT ON TABLE incident_notes IS
    'Append-only commentary on an incident. The API exposes no update or delete for notes — the original occurrence record is preserved intact for EASA traceability.';
