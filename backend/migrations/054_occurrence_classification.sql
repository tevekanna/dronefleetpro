-- 054_occurrence_classification.sql
-- EASA occurrence classification (Annex VI to IR (EU) 2015/1018, as added by
-- IR (EU) 2026/1821) and 72-hour reporting-deadline tracking on incidents.
-- Additive only — no data is modified or backfilled. In particular,
-- became_aware_at and reported_at are NOT derived from occurred_at or
-- reported_to_authority: inventing a timestamp that was never recorded would
-- fabricate evidence of regulatory compliance.

ALTER TABLE aircraft_models
    ADD COLUMN IF NOT EXISTS design_approval_required TEXT
        CHECK (design_approval_required IN ('required', 'not_required'));

COMMENT ON COLUMN aircraft_models.design_approval_required IS
    'Whether the design requires a certificate or declaration under Article 56(1) and (5) of Regulation (EU) 2018/1139, which is what makes Annex VI occurrence reporting mandatory. NULL means NOT DETERMINED and must never be treated as ''not_required''.';

ALTER TABLE incidents
    ADD COLUMN IF NOT EXISTS occurrence_code        TEXT,
    ADD COLUMN IF NOT EXISTS became_aware_at        TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS reported_at            TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS reporting_delay_reason TEXT;

COMMENT ON COLUMN incidents.occurrence_code IS
    'Annex VI occurrence code (e.g. VI.1.4.11), validated in Rust against services::occurrence_taxonomy. Deliberately has no foreign key: the taxonomy is a compile-time constant, not a table.';
COMMENT ON COLUMN incidents.became_aware_at IS
    'Start of the 72-hour reporting clock under Article 4(7) of Regulation (EU) No 376/2014. The clock runs from the moment the organisation became aware of the occurrence, NOT from occurred_at.';
COMMENT ON COLUMN incidents.reported_at IS
    'When the occurrence report was submitted to the competent authority. NULL until actually reported — never backfilled from reported_to_authority.';
COMMENT ON COLUMN incidents.reporting_delay_reason IS
    'Records the "unless exceptional circumstances prevent this" carve-out in Article 4(7) of Regulation (EU) No 376/2014 when the 72-hour deadline could not be met.';
