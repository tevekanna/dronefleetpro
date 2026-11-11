CREATE TABLE model_documents (
    id                  SERIAL PRIMARY KEY,
    model_id            INTEGER NOT NULL REFERENCES aircraft_models(id) ON DELETE CASCADE,
    org_id              INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    category            TEXT NOT NULL CHECK (category IN ('instruction_manual', 'maintenance_manual', 'other')),
    original_filename   TEXT NOT NULL,
    file_path           TEXT NOT NULL,
    size_bytes          BIGINT NOT NULL CHECK (size_bytes > 0),
    mime_type           TEXT NOT NULL,
    uploaded_by         INTEGER NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    uploaded_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (model_id, category)
);

CREATE INDEX idx_model_documents_model_id ON model_documents (model_id);
CREATE INDEX idx_model_documents_org_id ON model_documents (org_id);

COMMENT ON TABLE model_documents IS
    'Per-model document attachments (PDF only): instruction manual, maintenance manual, or other reference documentation. Replace semantics: UNIQUE(model_id, category) ensures at most one document per category per model. File content lives under ${UPLOAD_DIR}/aircraft_models/{model_id}/{document_id}.pdf.';
