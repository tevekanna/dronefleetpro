-- Drop the single-category uniqueness
ALTER TABLE model_documents DROP CONSTRAINT model_documents_model_id_category_key;

-- Add title column (NULL by default)
ALTER TABLE model_documents ADD COLUMN title TEXT;

-- Backfill title for any existing 'other' rows from original_filename
UPDATE model_documents
   SET title = regexp_replace(original_filename, '\.[^.]+$', '')
 WHERE category = 'other' AND title IS NULL;

-- Enforce title semantics: required for 'other', forbidden for the two manual
-- categories. Category label is identifier enough for manuals since the
-- partial unique index below restricts them to one row per (model, category).
ALTER TABLE model_documents ADD CONSTRAINT model_documents_title_required_for_other
  CHECK (
    (category = 'other' AND title IS NOT NULL AND length(trim(title)) > 0)
    OR (category != 'other' AND title IS NULL)
  );

-- Partial unique index: at most one document per non-'other' category per
-- model. Multiple 'other' documents per model are allowed.
CREATE UNIQUE INDEX model_documents_model_category_unique_idx
  ON model_documents (model_id, category)
  WHERE category != 'other';

COMMENT ON COLUMN model_documents.title IS
    'Operator-supplied title. Required for category=other (where multiple documents are allowed per model and we need a way to tell them apart). Must be NULL for instruction_manual and maintenance_manual since their category label is identifier enough and replace semantics enforce single-row-per-category.';
